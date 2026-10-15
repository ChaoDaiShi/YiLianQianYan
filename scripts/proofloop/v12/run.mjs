import {randomUUID} from 'node:crypto';
import {access, mkdir, readFile, realpath, writeFile} from 'node:fs/promises';
import {dirname, extname, isAbsolute, join, relative, resolve, sep} from 'node:path';
import {fileURLToPath} from 'node:url';
import {
  appendJsonLine, canonicalJson, isWithin, parseArguments, readJson, repositoryRoot,
  schemaHash, sha256, sourceCommit, sourceTreeDirty,
} from './shared.mjs';

const DEFAULT_MANIFEST = 'scripts/proofloop/v12/manifest.json';
const API_TIMEOUT_MS = 30_000;
const TERMINAL_EXECUTION = new Set(['succeeded', 'failed', 'cancelled', 'stale']);

class EvaluationFault extends Error {
  constructor(message, {status = 'FAIL', details = null, environment = false} = {}) {
    super(message);
    this.name = 'EvaluationFault';
    this.status = status;
    this.details = details;
    this.environment = environment;
  }
}

function apiUrl(base, path) {
  const normalized = `${base.replace(/\/+$/, '')}/`;
  return new URL(path.replace(/^\//, ''), normalized);
}

async function apiRequest(instance, method, path, body, {accepted, sse = false, raw = false, timeoutMs = API_TIMEOUT_MS} = {}) {
  let response;
  try {
    response = await fetch(apiUrl(instance.apiBase, path), {
      method,
      headers: {
        'X-Yilian-Control-Session': instance.controlToken,
        Accept: sse ? 'text/event-stream' : 'application/json',
        ...(body === undefined ? {} : {'Content-Type': 'application/json'}),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
  } catch (error) {
    throw new EvaluationFault(`api_transport_failed:${method}:${path}`, {
      status: 'BLOCKED_ENV', environment: true, details: {name: error.name, code: error.code ?? null},
    });
  }

  if (raw) {
    if (!response.ok) throw new EvaluationFault(`api_http_${response.status}:${method}:${path}`, {status: response.status >= 500 ? 'BLOCKED_ENV' : 'FAIL', environment: response.status >= 500, details: {http_status: response.status}});
    return {status: response.status, payload: Buffer.from(await response.arrayBuffer()), headers: response.headers};
  }
  const text = await response.text();
  let payload = text;
  if (!sse && text) {
    try { payload = JSON.parse(text); }
    catch { payload = {unparsed_response: text.slice(0, 4000)}; }
  }
  if (accepted && accepted.includes(response.status)) return {status: response.status, payload, headers: response.headers};
  if (!response.ok) {
    const environment = response.status === 401 || response.status === 403 || response.status >= 500;
    throw new EvaluationFault(`api_http_${response.status}:${method}:${path}`, {
      status: environment ? 'BLOCKED_ENV' : 'FAIL', environment,
      details: {http_status: response.status, response: safePayload(payload)},
    });
  }
  return {status: response.status, payload, headers: response.headers};
}

function safePayload(value) {
  if (typeof value === 'string') return value.slice(0, 4000).replace(/Bearer\s+\S+/gi, 'Bearer [REDACTED_SECRET]');
  if (Array.isArray(value)) return value.map(safePayload);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [
    key, /key|token|secret|password|authorization|control_session/i.test(key) ? '[REDACTED_SECRET]' : safePayload(item),
  ]));
}

function requireString(value, name) {
  if (typeof value !== 'string' || !value.trim()) throw new EvaluationFault(`missing_or_invalid:${name}`, {status: 'BLOCKED_ENV', environment: true});
  return value.trim();
}

function checkApiBase(value, group) {
  let url;
  try { url = new URL(value); }
  catch { throw new EvaluationFault(`${group}:invalid_api_base`, {status: 'BLOCKED_ENV', environment: true}); }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
    throw new EvaluationFault(`${group}:api_base_must_not_embed_credentials_or_query`, {status: 'BLOCKED_ENV', environment: true});
  }
  if (url.protocol === 'http:' && !['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)) {
    throw new EvaluationFault(`${group}:remote_api_requires_https`, {status: 'BLOCKED_ENV', environment: true});
  }
  return url.toString().replace(/\/$/, '');
}

async function listCapabilities(instance) {
  const capabilities = [];
  let offset = 0;
  let expectedTotal = null;
  for (;;) {
    const {payload} = await apiRequest(instance, 'GET', `/api/capabilities?limit=500&offset=${offset}`);
    if (!Array.isArray(payload.capabilities) || !Number.isInteger(payload.total)) {
      throw new EvaluationFault('capability_registry_response_invalid', {status: 'BLOCKED_ENV', environment: true});
    }
    expectedTotal ??= payload.total;
    capabilities.push(...payload.capabilities);
    if (capabilities.length >= expectedTotal) break;
    if (payload.capabilities.length === 0 || offset > 1_000_000) {
      throw new EvaluationFault('capability_registry_pagination_incomplete', {status: 'BLOCKED_ENV', environment: true});
    }
    offset += payload.capabilities.length;
  }
  if (capabilities.length !== expectedTotal) throw new EvaluationFault('capability_registry_total_mismatch', {status: 'BLOCKED_ENV', environment: true});
  return capabilities;
}

async function loadSkills(instance) {
  const {payload: listed} = await apiRequest(instance, 'GET', '/api/skills');
  if (!Array.isArray(listed)) throw new EvaluationFault('skills_response_invalid', {status: 'BLOCKED_ENV', environment: true});
  const skills = [];
  for (const skill of [...listed].sort((a, b) => String(a.name).localeCompare(String(b.name)))) {
    const name = requireString(skill.name, 'skill.name');
    const {payload} = await apiRequest(instance, 'GET', `/api/skills/${encodeURIComponent(name)}`);
    if (typeof payload.content !== 'string' || payload.error) {
      throw new EvaluationFault(`skill_content_unavailable:${name}`, {status: 'BLOCKED_ENV', environment: true});
    }
    skills.push({name, description: skill.description ?? '', content: payload.content});
  }
  return skills;
}

function normalizeBindings(document, descriptors, tasks) {
  if (document.schema_version !== 1 || !document.bindings || typeof document.bindings !== 'object') {
    throw new EvaluationFault('task_bindings_schema_invalid', {status: 'BLOCKED_ENV', environment: true});
  }
  const byId = new Map(descriptors.map(item => [String(item.id), item]));
  const resolved = {};
  const errors = [];
  const required = [...new Set(tasks.flatMap(task => task.allowed_capability_ids ?? []))].sort();
  for (const logicalId of required) {
    const binding = document.bindings[logicalId];
    if (!binding || typeof binding.capability_id !== 'string' || typeof binding.schema_sha256 !== 'string') {
      errors.push(`binding_missing:${logicalId}`); continue;
    }
    const descriptor = byId.get(binding.capability_id);
    if (!descriptor) { errors.push(`capability_missing:${logicalId}:${binding.capability_id}`); continue; }
    const capabilityIdentity = `${descriptor.name ?? ''} ${descriptor.description ?? ''} ${descriptor.metadata?.source_name ?? ''} ${JSON.stringify(descriptor.metadata?.extra ?? {})}`;
    if (/controlled\s+fixture|mock\s+(?:server|tool|provider)|stub\s+(?:server|tool|provider)|fake\s+(?:server|tool|provider)|loopback\s+(?:fixture|model)/i.test(capabilityIdentity)) {
      errors.push(`controlled_fixture_capability_rejected:${logicalId}`);
    }
    const actualHash = schemaHash(descriptor);
    if (actualHash !== binding.schema_sha256) errors.push(`capability_schema_changed:${logicalId}`);
    if (String(descriptor.status).toLowerCase() !== 'ready' || descriptor.enabled !== true || descriptor.metadata?.runtime_ready !== true) {
      errors.push(`capability_not_ready:${logicalId}`);
    }
    if (!['mcp_tool', 'workflow'].includes(String(descriptor.kind).toLowerCase())) errors.push(`capability_kind_not_supported:${logicalId}`);
    resolved[logicalId] = {
      capability_id: binding.capability_id,
      schema_sha256: actualHash,
      descriptor,
    };
  }
  if (errors.length) throw new EvaluationFault('task_binding_preflight_failed', {status: 'BLOCKED_ENV', environment: true, details: errors});
  return resolved;
}

function deriveExternalIdentity(instanceConfig, currentCommit, packageVersion) {
  const binaryHash = typeof instanceConfig.binary_sha256 === 'string' ? instanceConfig.binary_sha256.toLowerCase() : null;
  const mappedCommit = typeof instanceConfig.binary_source_commit === 'string' ? instanceConfig.binary_source_commit : null;
  const validHash = binaryHash && /^[a-f0-9]{64}$/.test(binaryHash);
  const mapped = Boolean(validHash && mappedCommit === currentCommit);
  return {
    local_source_commit: currentCommit,
    api_package_version: packageVersion,
    api_commit_reported: false,
    server_commit_verified: false,
    external_binary_sha256: validHash ? binaryHash : null,
    external_binary_source_commit: mappedCommit,
    external_binary_source_mapping_matches_local: mapped,
    publishable_g4_identity: mapped,
  };
}

export async function preflight(manifestPath = DEFAULT_MANIFEST) {
  const manifestFile = resolveFromRoot(manifestPath);
  const manifest = await readJson(manifestFile);
  const tasksFile = resolveFromRoot(manifest.source_task_file);
  const tasksDocument = await readJson(tasksFile);
  const tasks = tasksDocument.tasks;
  if (!Array.isArray(tasks) || tasks.length !== manifest.task_count || tasks.some(task => !task.task_id)) {
    throw new EvaluationFault('frozen_task_set_invalid', {status: 'BLOCKED_ENV', environment: true});
  }
  if (new Set(tasks.map(task => task.task_id)).size !== manifest.task_count) {
    throw new EvaluationFault('duplicate_frozen_task_id', {status: 'BLOCKED_ENV', environment: true});
  }
  if (!Array.isArray(manifest.task_ids) || canonicalJson(manifest.task_ids) !== canonicalJson(tasks.map(task => task.task_id))) {
    throw new EvaluationFault('manifest_task_ids_do_not_match_frozen_task_order', {status: 'BLOCKED_ENV', environment: true});
  }
  const byTaskId = new Map(tasks.map(task => [task.task_id, task]));
  if (!Array.isArray(manifest.pilot_task_ids) || manifest.pilot_task_ids.length !== 3 || new Set(manifest.pilot_task_ids).size !== 3 ||
      manifest.pilot_task_ids.some(taskId => !byTaskId.has(taskId)) || new Set(manifest.pilot_task_ids.map(taskId => byTaskId.get(taskId).category)).size !== 3) {
    throw new EvaluationFault('manifest_pilot_tasks_must_cover_each_frozen_category', {status: 'BLOCKED_ENV', environment: true});
  }
  const taskHash = sha256(await readFile(tasksFile));
  if (manifest.task_sha256 !== taskHash) throw new EvaluationFault('frozen_task_hash_mismatch', {status: 'BLOCKED_ENV', environment: true});

  const requiredEnvironment = manifest.required_environment ?? [];
  const missing = requiredEnvironment.filter(name => !process.env[name]?.trim());
  if (missing.length) throw new EvaluationFault('required_environment_missing', {status: 'BLOCKED_ENV', environment: true, details: missing});
  const unauthorized = ['YILIAN_V12_BUDGET_AUTHORIZED', 'YILIAN_V12_SIDE_EFFECTS_AUTHORIZED'].filter(name => process.env[name] !== '1');
  if (unauthorized.length) throw new EvaluationFault('explicit_run_authorization_missing', {status: 'BLOCKED_ENV', environment: true, details: unauthorized});
  if (sourceTreeDirty()) throw new EvaluationFault('source_tree_dirty_commit_before_evaluation', {status: 'BLOCKED_ENV', environment: true});

  if (!isAbsolute(process.env.YILIAN_V12_GROUP_INSTANCES_FILE) || !isAbsolute(process.env.YILIAN_V12_TASK_BINDINGS)) {
    throw new EvaluationFault('group_instance_and_task_binding_files_must_be_absolute', {status: 'BLOCKED_ENV', environment: true});
  }
  const groupFile = resolve(process.env.YILIAN_V12_GROUP_INSTANCES_FILE);
  const instancesDocument = await readJson(groupFile);
  if (instancesDocument.schema_version !== 1 || !instancesDocument.groups) {
    throw new EvaluationFault('group_instances_schema_invalid', {status: 'BLOCKED_ENV', environment: true});
  }
  const bindingsFile = resolve(process.env.YILIAN_V12_TASK_BINDINGS);
  const taskBindings = await readJson(bindingsFile);
  if (!isAbsolute(process.env.YILIAN_V12_ISOLATED_PROFILE)) throw new EvaluationFault('YILIAN_V12_ISOLATED_PROFILE_must_be_absolute', {status: 'BLOCKED_ENV', environment: true});
  const isolatedProfile = resolve(process.env.YILIAN_V12_ISOLATED_PROFILE);
  if (!await isDirectory(isolatedProfile)) throw new EvaluationFault('isolated_profile_root_missing', {status: 'BLOCKED_ENV', environment: true});
  const currentCommit = sourceCommit();
  const groupRuntime = {};
  const publicGroups = {};
  const errors = [];
  const profilePaths = new Set();
  const endpointPaths = new Set();
  let commonBindings = null;
  let commonCapabilityHash = null;
  let commonSkillHash = null;
  let commonModelIdentity = null;
  let commonPricing = null;
  let commonPackageVersion = null;
  let commonBinaryHash = null;

  for (const [groupName, groupSpec] of Object.entries(manifest.groups)) {
    try {
      const groupConfig = instancesDocument.groups[groupName];
      if (!groupConfig) throw new EvaluationFault(`${groupName}:group_instance_missing`, {status: 'BLOCKED_ENV', environment: true});
      const profileRelative = requireString(groupConfig.profile_dir, `${groupName}.profile_dir`);
      const profileDir = resolve(isolatedProfile, profileRelative);
      if (!isWithin(isolatedProfile, profileDir) || profileDir === isolatedProfile || !await isDirectory(profileDir)) {
        throw new EvaluationFault(`${groupName}:profile_directory_invalid`, {status: 'BLOCKED_ENV', environment: true});
      }
      const canonicalIsolatedProfile = await realpath(isolatedProfile);
      const canonicalProfile = await realpath(profileDir);
      if (!isWithin(canonicalIsolatedProfile, canonicalProfile)) throw new EvaluationFault(`${groupName}:profile_symlink_escapes_isolated_root`, {status: 'BLOCKED_ENV', environment: true});
      const profileKey = process.platform === 'win32' ? canonicalProfile.toLowerCase() : canonicalProfile;
      if (profilePaths.has(profileKey)) throw new EvaluationFault(`${groupName}:group_profiles_must_be_distinct`, {status: 'BLOCKED_ENV', environment: true});
      profilePaths.add(profileKey);

      const workspaceRelative = groupConfig.workspace_root ?? 'workspaces';
      const workspaceRoot = resolve(canonicalProfile, requireString(workspaceRelative, `${groupName}.workspace_root`));
      if (!isWithin(canonicalProfile, workspaceRoot) || workspaceRoot === canonicalProfile) {
        throw new EvaluationFault(`${groupName}:workspace_root_must_be_within_isolated_profile`, {status: 'BLOCKED_ENV', environment: true});
      }

      const apiBase = checkApiBase(requireString(groupConfig.api_base, `${groupName}.api_base`), groupName);
      if (endpointPaths.has(apiBase)) throw new EvaluationFault(`${groupName}:group_api_endpoints_must_be_distinct`, {status: 'BLOCKED_ENV', environment: true});
      endpointPaths.add(apiBase);
      const tokenEnv = requireString(groupConfig.control_session_env, `${groupName}.control_session_env`);
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(tokenEnv)) throw new EvaluationFault(`${groupName}:invalid_control_session_env_name`, {status: 'BLOCKED_ENV', environment: true});
      const controlToken = requireString(process.env[tokenEnv], `${groupName}.${tokenEnv}`);
      const modelId = requireString(groupConfig.model_id, `${groupName}.model_id`);
      const instance = {apiBase, controlToken, groupName, profileDir, workspaceRoot, modelId};

      const {payload: health} = await apiRequest(instance, 'GET', '/api/health');
      if (health.status !== 'healthy' || health.database !== 'healthy' || typeof health.version !== 'string') {
        throw new EvaluationFault(`${groupName}:backend_health_not_ready`, {status: 'BLOCKED_ENV', environment: true, details: safePayload(health)});
      }
      if (commonPackageVersion !== null && commonPackageVersion !== health.version) throw new EvaluationFault(`${groupName}:backend_package_versions_differ_between_groups`, {status: 'BLOCKED_ENV', environment: true});
      commonPackageVersion ??= health.version;
      const settingsResponse = await apiRequest(instance, 'GET', '/api/settings');
      const actualAiRepair = settingsResponse.payload?.ai_repair_enabled;
      if (typeof actualAiRepair !== 'boolean' || actualAiRepair !== groupSpec.ai_repair) {
        throw new EvaluationFault(`${groupName}:ai_repair_setting_does_not_match_manifest`, {status: 'BLOCKED_ENV', environment: true, details: {expected: groupSpec.ai_repair, actual: actualAiRepair ?? null}});
      }

      const {payload: modelRows} = await apiRequest(instance, 'GET', '/api/llm/models');
      if (!Array.isArray(modelRows)) throw new EvaluationFault(`${groupName}:model_list_invalid`, {status: 'BLOCKED_ENV', environment: true});
      const model = modelRows.find(item => item.id === modelId);
      if (!model || !model.active || !model.api_key_configured || !model.verified_at || !model.provider || !model.model) {
        throw new EvaluationFault(`${groupName}:configured_real_model_not_active_verified`, {status: 'BLOCKED_ENV', environment: true});
      }
      if (/fixture|mock|stub|fake|loopback/i.test(`${model.provider} ${model.model} ${model.base_url ?? ''}`)) {
        throw new EvaluationFault(`${groupName}:controlled_model_fixture_is_not_real_model_evidence`, {status: 'BLOCKED_ENV', environment: true});
      }
      const activeModels = modelRows.filter(item => item.active);
      if (activeModels.length !== 1 || activeModels[0].id !== modelId) {
        throw new EvaluationFault(`${groupName}:expected_exactly_one_active_model`, {status: 'BLOCKED_ENV', environment: true});
      }
      const modelIdentity = {
        provider: model.provider, model: model.model, api_format: model.api_format,
        temperature: model.temperature, max_tokens: model.max_tokens, invoke_timeout_ms: model.invoke_timeout_ms,
      };
      const pricing = normalizePricing(groupConfig.model_pricing, groupName);
      if (commonModelIdentity !== null && canonicalJson(modelIdentity) !== commonModelIdentity) {
        throw new EvaluationFault(`${groupName}:model_profile_differs_between_groups`, {status: 'BLOCKED_ENV', environment: true});
      }
      if (commonPricing !== null && canonicalJson(pricing) !== commonPricing) {
        throw new EvaluationFault(`${groupName}:model_pricing_differs_between_groups`, {status: 'BLOCKED_ENV', environment: true});
      }
      commonModelIdentity ??= canonicalJson(modelIdentity);
      commonPricing ??= canonicalJson(pricing);

      const capabilities = await listCapabilities(instance);
      const registryHash = sha256(canonicalJson(capabilities));
      if (commonCapabilityHash !== null && registryHash !== commonCapabilityHash) {
        throw new EvaluationFault(`${groupName}:capability_registry_differs_between_groups`, {status: 'BLOCKED_ENV', environment: true});
      }
      commonCapabilityHash ??= registryHash;
      const resolved = normalizeBindings(taskBindings, capabilities, tasks);
      const logicalBindingSnapshot = Object.fromEntries(Object.entries(resolved).sort(([a], [b]) => a.localeCompare(b)).map(([key, binding]) => [key, {capability_id: binding.capability_id, schema_sha256: binding.schema_sha256}]));
      const bindingFingerprint = canonicalJson(logicalBindingSnapshot);
      if (commonBindings !== null && commonBindings !== bindingFingerprint) {
        throw new EvaluationFault(`${groupName}:capability_bindings_differ_between_groups`, {status: 'BLOCKED_ENV', environment: true});
      }
      commonBindings ??= bindingFingerprint;

      const skills = await loadSkills(instance);
      const skillsHash = sha256(canonicalJson(skills));
      if (groupSpec.experience && skills.length === 0) throw new EvaluationFault(`${groupName}:experience_group_requires_real_skill_corpus`, {status: 'BLOCKED_ENV', environment: true});
      if (commonSkillHash !== null && skillsHash !== commonSkillHash) {
        throw new EvaluationFault(`${groupName}:skill_corpus_differs_between_groups`, {status: 'BLOCKED_ENV', environment: true});
      }
      commonSkillHash ??= skillsHash;
      const probe = await apiRequest(instance, 'POST', '/api/task-world/proofloop/drafts/__v12_preflight_probe__/select-candidate', {}, {accepted: [400, 409, 422]});
      if (![400, 409, 422].includes(probe.status)) {
        throw new EvaluationFault(`${groupName}:select_candidate_route_unavailable`, {status: 'BLOCKED_ENV', environment: true, details: {http_status: probe.status}});
      }
      const localHealth = await apiRequest(instance, 'GET', '/api/health');
      const candidateIdentity = deriveExternalIdentity(groupConfig, currentCommit, localHealth.payload.version);
      if (candidateIdentity.publishable_g4_identity) {
        if (commonBinaryHash !== null && commonBinaryHash !== candidateIdentity.external_binary_sha256) {
          throw new EvaluationFault(`${groupName}:backend_binary_hash_differs_between_groups`, {status: 'BLOCKED_ENV', environment: true});
        }
        commonBinaryHash ??= candidateIdentity.external_binary_sha256;
      }
      const publicConfig = {
        api_base: apiBase,
        profile_dir: profileRelative,
        workspace_root: relative(profileDir, workspaceRoot),
        model_id: model.id,
        model: {
          provider: model.provider,
          model: model.model,
          api_format: model.api_format,
          temperature: model.temperature,
          max_tokens: model.max_tokens,
          invoke_timeout_ms: model.invoke_timeout_ms,
          verified_at: model.verified_at,
          api_key_configured: model.api_key_configured,
          seed_supported_by_current_product_api: false,
        },
        settings: {ai_repair_enabled: actualAiRepair},
        experiment_configuration: {strategy: groupSpec.strategy, candidate_count: groupSpec.candidate_count, experience_enabled: groupSpec.experience, ai_repair_enabled: groupSpec.ai_repair},
        package_version: health.version,
        task_bindings_hash: sha256(canonicalJson(taskBindings)),
        capability_registry_hash: registryHash,
        capability_count: capabilities.length,
        skill_corpus_hash: skillsHash,
        skill_count: skills.length,
        model_pricing: pricing,
        candidate_identity: candidateIdentity,
      };
      groupRuntime[groupName] = {...instance, groupSpec, groupConfig, model, capabilities, resolved, skills, publicConfig};
      publicGroups[groupName] = publicConfig;
    } catch (error) {
      errors.push({group: groupName, message: error.message, details: error.details ?? null});
    }
  }

  const configuredGroups = Object.keys(instancesDocument.groups).sort();
  const manifestGroups = Object.keys(manifest.groups).sort();
  if (canonicalJson(configuredGroups) !== canonicalJson(manifestGroups)) errors.push({message: 'group_instance_set_must_match_manifest'});
  if (errors.length) throw new EvaluationFault('preflight_blocked', {status: 'BLOCKED_ENV', environment: true, details: errors});
  const firstRuntime = groupRuntime[manifestGroups[0]];
  const taskBindingsEvidence = {
    schema_version: 1,
    bindings: Object.fromEntries(Object.entries(firstRuntime.resolved).map(([logicalId, binding]) => [logicalId, {
      capability_id: binding.capability_id,
      schema_sha256: binding.schema_sha256,
    }])),
  };

  return {
    status: 'PREFLIGHT_READY',
    benchmark_id: manifest.benchmark_id,
    source_commit: currentCommit,
    source_tree_dirty: false,
    source_task_sha256: taskHash,
    task_bindings_file_sha256: sha256(canonicalJson(taskBindings)),
    stage_requirements: {
      model_spend_requires_cli_flag_and_YILIAN_V12_BUDGET_AUTHORIZED_1: true,
      high_risk_task_tools_require_cli_flag_and_YILIAN_V12_SIDE_EFFECTS_AUTHORIZED_1: true,
      group_profiles_are_dedicated_and_unique: true,
      per_run_workspace_directory: true,
    },
    groups: publicGroups,
    publishable_g4_identity: Object.values(publicGroups).every(group => group.candidate_identity.publishable_g4_identity) && new Set(Object.values(publicGroups).map(group => group.candidate_identity.external_binary_sha256)).size === 1,
    tasks,
    taskBindings: taskBindingsEvidence,
    manifest,
    runtime: groupRuntime,
  };
}

function normalizePricing(value, groupName) {
  if (!value || typeof value !== 'object') throw new EvaluationFault(`${groupName}:model_pricing_source_missing`, {status: 'BLOCKED_ENV', environment: true});
  let source;
  try { source = new URL(requireString(value.source_url, `${groupName}.model_pricing.source_url`)); }
  catch { throw new EvaluationFault(`${groupName}:model_pricing_source_invalid`, {status: 'BLOCKED_ENV', environment: true}); }
  if (source.protocol !== 'https:' || source.username || source.password || source.search || source.hash || !Number.isFinite(value.observed_at_ms) || value.observed_at_ms <= 0 ||
      !Number.isFinite(value.input_per_million) || value.input_per_million < 0 || !Number.isFinite(value.output_per_million) || value.output_per_million < 0 ||
      typeof value.currency !== 'string' || !value.currency.trim()) {
    throw new EvaluationFault(`${groupName}:model_pricing_values_invalid`, {status: 'BLOCKED_ENV', environment: true});
  }
  return {
    source_url: source.toString(),
    observed_at_ms: value.observed_at_ms,
    currency: value.currency.trim().toUpperCase(),
    input_per_million: value.input_per_million,
    output_per_million: value.output_per_million,
  };
}

function resolveFromRoot(path) {
  return isAbsolute(path) ? resolve(path) : resolve(repositoryRoot, path);
}

async function isDirectory(path) {
  try { await access(path); return true; }
  catch { return false; }
}

function seededRandom(seed) {
  let value = seed >>> 0;
  return () => {
    value += 0x6D2B79F5;
    let next = value;
    next = Math.imul(next ^ (next >>> 15), next | 1);
    next ^= next + Math.imul(next ^ (next >>> 7), next | 61);
    return ((next ^ (next >>> 14)) >>> 0) / 4294967296;
  };
}

function shuffle(values, random) {
  const result = [...values];
  for (let index = result.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [result[index], result[swap]] = [result[swap], result[index]];
  }
  return result;
}

export function makeSchedule(stage, tasks, manifest) {
  const groups = Object.keys(manifest.groups).sort();
  const random = seededRandom(manifest.seed);
  const tasksById = new Map(tasks.map(task => [task.task_id, task]));
  const selectedTasks = stage === 'pilot' ? manifest.pilot_task_ids.map(taskId => tasksById.get(taskId)) : tasks;
  const repeats = stage === 'pilot' ? [1] : Array.from({length: manifest.runs_per_group}, (_, index) => index + 1);
  const entries = [];
  for (const task of selectedTasks) {
    for (const repeat of repeats) {
      const order = shuffle(groups, random);
      for (const group of order) entries.push({order: entries.length + 1, task_id: task.task_id, category: task.category, group, repeat});
    }
  }
  return {
    schema_version: 1,
    stage,
    seed: manifest.seed,
    randomization: 'groups shuffled within each task+repeat block; blocks retain frozen task order',
    task_ids: selectedTasks.map(task => task.task_id),
    planned_slots: entries.length,
    entries,
  };
}

function taskById(tasks) { return new Map(tasks.map(task => [task.task_id, task])); }

function runId(entry) { return `v12-${entry.task_id}-${entry.group}-r${entry.repeat}`; }

function normalizeStatus(value) { return String(value ?? '').toLowerCase(); }

function topologicalNodes(candidate) {
  const nodes = candidate?.nodes;
  if (!Array.isArray(nodes) || nodes.length === 0) throw new EvaluationFault('candidate_has_no_nodes');
  const byId = new Map(nodes.map(node => [node.id, node]));
  const incoming = new Map(nodes.map(node => [node.id, 0]));
  const outgoing = new Map(nodes.map(node => [node.id, []]));
  for (const edge of candidate.edges ?? []) {
    if (!byId.has(edge.from) || !byId.has(edge.to)) throw new EvaluationFault('candidate_edge_references_missing_node');
    incoming.set(edge.to, incoming.get(edge.to) + 1);
    outgoing.get(edge.from).push(edge.to);
  }
  const ready = nodes.filter(node => incoming.get(node.id) === 0).map(node => node.id);
  const ordered = [];
  while (ready.length) {
    const id = ready.shift(); ordered.push(byId.get(id));
    for (const next of outgoing.get(id)) {
      incoming.set(next, incoming.get(next) - 1);
      if (incoming.get(next) === 0) ready.push(next);
    }
  }
  if (ordered.length !== nodes.length) throw new EvaluationFault('candidate_graph_contains_cycle');
  return ordered;
}

function validateFrozenContract(contract, task, allowedCapabilityIds) {
  if (contract.user_goal !== task.user_goal) throw new EvaluationFault('frozen_user_goal_changed');
  if (canonicalJson(contract.deliverables) !== canonicalJson(task.deliverables)) throw new EvaluationFault('frozen_deliverables_changed');
  if (canonicalJson(contract.criteria) !== canonicalJson(task.criteria)) throw new EvaluationFault('frozen_acceptance_criteria_changed');
  if (canonicalJson(contract.constraints ?? []) !== canonicalJson([])) throw new EvaluationFault('frozen_constraints_changed');
  const expectedBudget = task.budget;
  for (const field of ['max_model_calls', 'max_total_tokens', 'max_wall_time_ms', 'max_repair_rounds']) {
    if (contract.budget?.[field] !== expectedBudget[field]) throw new EvaluationFault(`frozen_budget_changed:${field}`);
  }
  const actualCapabilities = [...new Set(contract.allowed_capability_ids ?? [])].sort();
  const expectedCapabilities = [...new Set(allowedCapabilityIds)].sort();
  if (canonicalJson(actualCapabilities) !== canonicalJson(expectedCapabilities)) throw new EvaluationFault('frozen_allowed_capability_set_changed');
}

function validateDeliverableMappings(candidate, task) {
  const expectedIds = (task.deliverables ?? []).map(item => item.id).sort();
  const mappings = candidate?.deliverable_nodes ?? [];
  const actualIds = mappings.map(item => item.deliverable_id).sort();
  if (new Set(actualIds).size !== actualIds.length || canonicalJson(actualIds) !== canonicalJson(expectedIds)) {
    throw new EvaluationFault('deliverable_node_mappings_do_not_match_frozen_task');
  }
  const candidateNodeIds = new Set((candidate.nodes ?? []).map(node => node.id));
  if (mappings.some(item => !candidateNodeIds.has(item.node_id))) throw new EvaluationFault('deliverable_mapping_references_unknown_node');
}

function eventLogger(path, runIdValue) {
  return async (phase, data = {}) => appendJsonLine(path, {at_ms: Date.now(), run_id: runIdValue, phase, ...safePayload(data)});
}

async function getGraphDetail(instance, graphId) {
  const {payload} = await apiRequest(instance, 'GET', `/api/task-world/graphs/${encodeURIComponent(graphId)}/detail`);
  if (!payload.detail?.graph_id || !Array.isArray(payload.detail.nodes)) throw new EvaluationFault('graph_detail_response_invalid');
  return payload.detail;
}

async function getExecutionSnapshot(instance, graphId, nodeId, executionId) {
  const query = new URLSearchParams({include_snapshot: 'true', execution_id: executionId});
  const {payload} = await apiRequest(instance, 'GET', `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/executions?${query}`);
  const summary = payload.executions?.find(item => item.execution_id === executionId);
  return {summary, snapshot: payload.execution_snapshot};
}

async function findPendingApproval(instance, graphId, nodeId, executionId) {
  const {payload} = await apiRequest(instance, 'GET', '/api/approvals/pending');
  return (payload.approvals ?? []).find(item =>
    item.task_graph_id === graphId && item.task_node_id === nodeId &&
    (item.node_execution_id === executionId || item.task_execution_id === executionId) && item.status === 'pending');
}

function parseSseStatuses(text) {
  const events = [];
  for (const block of text.split(/\r?\n\r?\n/)) {
    const data = block.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trim()).join('\n');
    if (!data) continue;
    try { events.push(JSON.parse(data)); }
    catch { events.push({unparsed_sse_data: data.slice(0, 1000)}); }
  }
  return events;
}

async function settleExecution(instance, graphId, nodeId, executionId, onEvent, timeoutMs) {
  const until = Date.now() + timeoutMs;
  let approvalHandled = false;
  while (Date.now() < until) {
    const {summary, snapshot} = await getExecutionSnapshot(instance, graphId, nodeId, executionId);
    if (!summary) throw new EvaluationFault(`execution_not_found:${executionId}`);
    const status = normalizeStatus(summary.status);
    if (TERMINAL_EXECUTION.has(status)) return {summary, snapshot};
    if (status === 'waiting_approval' && !approvalHandled) {
      const pending = await findPendingApproval(instance, graphId, nodeId, executionId);
      if (!pending) throw new EvaluationFault(`approval_not_found_for_execution:${executionId}`);
      const response = await apiRequest(instance, 'POST', `/api/approvals/${encodeURIComponent(pending.approval_id)}/approve`, {}, {sse: true, timeoutMs});
      const events = parseSseStatuses(response.payload);
      await onEvent('approval_sse', {approval_id: pending.approval_id, node_id: nodeId, execution_id: executionId, events});
      if (!events.some(event => event.type === 'approval_resolved' && normalizeStatus(event.status) === 'approved' || event.status?.status === 'approved')) {
        const {payload: approvalState} = await apiRequest(instance, 'GET', `/api/approvals/${encodeURIComponent(pending.approval_id)}`);
        const approval = approvalState.approval ?? approvalState;
        if (normalizeStatus(approval.status) !== 'approved') throw new EvaluationFault(`approval_sse_did_not_confirm_approval:${executionId}`, {details: {events, state: safePayload(approval)}});
      }
      approvalHandled = true;
      await onEvent('approval_resolved', {approval_id: pending.approval_id, node_id: nodeId, execution_id: executionId, actor: 'automated_test_driver'});
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new EvaluationFault(`execution_timeout:${executionId}`, {status: 'CANCELLED', details: {timeout_ms: timeoutMs}});
}

async function startNode(instance, graphId, nodeId, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs) {
  const detail = await getGraphDetail(instance, graphId);
  const {payload} = await apiRequest(instance, 'POST', `/api/task-world/graphs/${encodeURIComponent(graphId)}/nodes/${encodeURIComponent(nodeId)}/executions`, {expected_revision: detail.revision});
  const initial = payload.execution;
  if (!initial?.execution_id) throw new EvaluationFault(`execution_start_response_invalid:${nodeId}`);
  attempts.push(initial.execution_id);
  const settled = await settleExecution(instance, graphId, nodeId, initial.execution_id, onEvent, timeoutMs);
  const state = {node_id: nodeId, execution_id: initial.execution_id, status: normalizeStatus(settled.summary.status), summary: settled.summary, snapshot: settled.snapshot};
  await appendJsonLine(executionSnapshotsFile, safePayload({node_id: nodeId, execution_id: initial.execution_id, summary: settled.summary, snapshot: settled.snapshot}));
  latestByNode.set(nodeId, state);
  executionStates.set(initial.execution_id, state);
  await onEvent('node_execution_settled', {node_id: nodeId, execution_id: initial.execution_id, status: settled.summary.status, attempt: settled.summary.attempt});
  return latestByNode.get(nodeId);
}

async function runInitialGraph(instance, graphId, candidate, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs) {
  const nodes = topologicalNodes(candidate);
  const dependencies = new Map(nodes.map(node => [node.id, []]));
  for (const edge of candidate.edges ?? []) dependencies.get(edge.to).push(edge.from);
  for (const node of nodes) {
    if (dependencies.get(node.id).some(parent => latestByNode.get(parent)?.status !== 'succeeded')) {
      await onEvent('node_skipped_due_to_dependency', {node_id: node.id, dependency_ids: dependencies.get(node.id)});
      continue;
    }
    await startNode(instance, graphId, node.id, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs);
  }
  return nodes;
}

function mimeFormat(mime) {
  const mapping = {
    'text/markdown': 'markdown',
    'application/json': 'json',
  };
  const result = mapping[mime];
  if (!result) throw new EvaluationFault(`unsupported_benchmark_mime_type:${mime}`);
  return result;
}

async function materializeDeliverables(instance, outputDirectory, runDirectory, contract, graphId, workspaceId, candidate, latestByNode, runRow, onEvent) {
  const files = [];
  runRow.artifact_files = files;
  runRow.artifact_attempts ??= [];
  const nodeFor = new Map((candidate.deliverable_nodes ?? []).map(mapping => [mapping.deliverable_id, mapping.node_id]));
  for (const deliverable of contract.deliverables ?? []) {
    const nodeId = nodeFor.get(deliverable.id);
    const sourceExecution = nodeId ? latestByNode.get(nodeId) : null;
    if (!nodeId || sourceExecution?.status !== 'succeeded') {
      await onEvent('deliverable_not_materialized', {deliverable_id: deliverable.id, node_id: nodeId ?? null, reason: 'no_successful_current_attempt'});
      continue;
    }
    const mime = deliverable.mime_type;
    const {payload} = await apiRequest(instance, 'POST', '/api/artifacts/from-execution', {
      source: {kind: 'node', graph_id: graphId, node_id: nodeId, execution_id: sourceExecution.execution_id, workspace_id: workspaceId},
      name: deliverable.title,
      format: mimeFormat(mime),
      mime_type: mime,
    });
    const artifact = payload.artifact;
    if (!artifact?.id || artifact.mime_type !== mime || artifact.name !== deliverable.title) {
      throw new EvaluationFault(`materialized_artifact_metadata_mismatch:${deliverable.id}`, {details: safePayload(artifact)});
    }
    const response = await apiRequest(instance, 'GET', `/api/artifacts/${encodeURIComponent(artifact.id)}/download`, undefined, {raw: true});
    const bytes = response.payload;
    const headerMime = response.headers.get('content-type')?.split(';')[0].trim();
    if (headerMime !== mime) throw new EvaluationFault(`download_mime_mismatch:${deliverable.id}`, {details: {expected: mime, actual: headerMime}});
    const safeExecutionId = sourceExecution.execution_id.replace(/[^A-Za-z0-9_-]/g, '_');
    const fileName = `${deliverable.id}-${safeExecutionId}${extname(deliverable.title) || (mime === 'application/json' ? '.json' : '.md')}`;
    const artifactDirectory = join(runDirectory, 'artifacts');
    await mkdir(artifactDirectory, {recursive: true});
    const fullPath = join(artifactDirectory, fileName);
    await writeFile(fullPath, bytes, {flag: 'wx'});
    const relativePath = relative(outputDirectory, fullPath).split(sep).join('/');
    const file = {
      deliverable_id: deliverable.id,
      name: deliverable.title,
      path: relativePath,
      artifact_id: artifact.id,
      mime_type: mime,
      download_content_type: headerMime,
      size_bytes: bytes.length,
      sha256: sha256(bytes),
      source_graph_id: graphId,
      source_node_id: nodeId,
      source_execution_id: sourceExecution.execution_id,
      provenance: safePayload(payload.provenance ?? null),
    };
    files.push(file);
    runRow.artifact_attempts.push(file);
    runRow.evidence_files.push(relativePath);
    await onEvent('artifact_downloaded', {deliverable_id: deliverable.id, artifact_id: artifact.id, source_execution_id: sourceExecution.execution_id, size_bytes: bytes.length, sha256: file.sha256});
  }
  return files;
}

async function evaluateGoal(instance, goalId, graphId, goalVersion, executionIds, onEvent) {
  const detail = await getGraphDetail(instance, graphId);
  const {payload} = await apiRequest(instance, 'POST', `/api/task-world/proofloop/goals/${encodeURIComponent(goalId)}/evaluations`, {
    goal_version: goalVersion,
    graph_revision: detail.revision,
    execution_ids: executionIds,
  });
  await onEvent('product_evaluation_created', {report_id: payload.report_id, product_status: payload.status, graph_revision: payload.graph_revision});
  const {payload: evidence} = await apiRequest(instance, 'GET', `/api/task-world/proofloop/evaluations/${encodeURIComponent(payload.report_id)}`);
  return {report: payload, snapshot: evidence.snapshot, graph_revision: detail.revision};
}

async function saveEvaluationRound(runRow, outputDirectory, runDirectory, round, artifacts, executionIds, evaluation) {
  const roundsDirectory = join(runDirectory, 'evaluations');
  await mkdir(roundsDirectory, {recursive: true});
  const roundName = `round-${String(round).padStart(2, '0')}`;
  const reportPath = join(roundsDirectory, `${roundName}-report.json`);
  const snapshotPath = join(roundsDirectory, `${roundName}-snapshot.json`);
  await writeFile(reportPath, JSON.stringify(safePayload(evaluation.report), null, 2));
  await writeFile(snapshotPath, JSON.stringify(safePayload(evaluation.snapshot), null, 2));
  runRow.evaluation_file = relative(outputDirectory, reportPath).split(sep).join('/');
  runRow.evaluation_snapshot_file = relative(outputDirectory, snapshotPath).split(sep).join('/');
  runRow.product_claimed_pass = normalizeStatus(evaluation.report.status) === 'pass';
  runRow.product_report_id = evaluation.report.report_id;
  runRow.product_report_status = evaluation.report.status;
  runRow.product_report_results = safePayload(evaluation.report.results ?? []);
  runRow.artifact_files = artifacts;
  runRow.execution_ids = executionIds;
  runRow.graph_revision = evaluation.graph_revision;
  for (const path of [runRow.evaluation_file, runRow.evaluation_snapshot_file]) {
    if (!runRow.evidence_files.includes(path)) runRow.evidence_files.push(path);
  }
  return {reportPath, snapshotPath};
}

async function evaluateWithRepairs(instance, runRow, outputDirectory, runDirectory, graphId, goalId, goalVersion, workspaceId, candidate, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs) {
  let artifacts = await materializeDeliverables(instance, outputDirectory, runDirectory, runRow.contract, graphId, workspaceId, candidate, latestByNode, runRow, onEvent);
  let executionIds = [...latestByNode.values()].map(item => item.execution_id);
  let evaluation = await evaluateGoal(instance, goalId, graphId, goalVersion, executionIds, onEvent);
  let evaluationEvidence = await saveEvaluationRound(runRow, outputDirectory, runDirectory, 0, artifacts, executionIds, evaluation);
  for (let round = 0; round < runRow.max_repair_rounds && normalizeStatus(evaluation.report.status) !== 'pass'; round += 1) {
    const failed = [...latestByNode.values()].filter(item => item.status === 'failed').map(item => item.execution_id);
    if (failed.length === 0) break;
    const proposalResult = await apiRequest(instance, 'POST', `/api/task-world/proofloop/goals/${encodeURIComponent(goalId)}/repairs`, {
      report_id: evaluation.report.report_id,
      failure_execution_ids: failed,
    });
    const proposal = proposalResult.payload;
    if (!proposal.repair_id || !proposal.rerun_root_node_id || !Number.isInteger(proposal.expected_graph_revision)) {
      await onEvent('repair_proposal_unusable', {round: round + 1, response: safePayload(proposal)});
      break;
    }
    runRow.repair_rounds = (runRow.repair_rounds ?? 0) + 1;
    if (proposal.ai_repair_usage || proposal.ai_usage || proposal.repair_usage) {
      const usage = proposal.ai_repair_usage ?? proposal.ai_usage ?? proposal.repair_usage;
      const calls = usage.model_calls;
      const tokens = usage.total_tokens;
      if (Number.isInteger(calls) && Number.isInteger(runRow.model_calls)) runRow.model_calls += calls;
      else runRow.model_calls = null;
      if (Number.isFinite(tokens) && Number.isFinite(runRow.direct_total_tokens)) runRow.direct_total_tokens += tokens;
      else runRow.direct_total_tokens = null;
    } else if (instance.groupSpec.ai_repair) {
      runRow.model_calls = null;
      runRow.unattributed_ai_repair_calls = true;
    }
    await onEvent('repair_proposed', {round: round + 1, repair_id: proposal.repair_id, ai_repair_enabled: instance.groupSpec.ai_repair, failure_execution_ids: failed, rationale: proposal.rationale ?? null});
    const applied = await apiRequest(instance, 'POST', `/api/task-world/proofloop/repairs/${encodeURIComponent(proposal.repair_id)}/apply`, {expected_graph_revision: proposal.expected_graph_revision});
    const affected = applied.payload.affected_node_ids;
    if (!Array.isArray(affected) || !affected.includes(proposal.rerun_root_node_id)) {
      throw new EvaluationFault('repair_affected_nodes_invalid', {details: safePayload(applied.payload)});
    }
    await onEvent('repair_applied', {round: round + 1, repair_id: proposal.repair_id, graph_revision: applied.payload.graph_revision, affected_node_ids: affected});

    const beforeRoot = latestByNode.get(proposal.rerun_root_node_id)?.execution_id ?? null;
    const root = await waitForReplacementExecution(instance, graphId, proposal.rerun_root_node_id, beforeRoot, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs);
    if (root.status === 'succeeded') {
      const order = topologicalNodes(candidate).filter(node => affected.includes(node.id) && node.id !== proposal.rerun_root_node_id);
      for (const node of order) {
        const depsReady = (candidate.edges ?? []).filter(edge => edge.to === node.id).every(edge => latestByNode.get(edge.from)?.status === 'succeeded');
        if (!depsReady) {
          await onEvent('repair_node_skipped_due_to_dependency', {node_id: node.id, round: round + 1});
          continue;
        }
        await startNode(instance, graphId, node.id, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs);
      }
    }
    artifacts = await materializeDeliverables(instance, outputDirectory, runDirectory, runRow.contract, graphId, workspaceId, candidate, latestByNode, runRow, onEvent);
    executionIds = [...latestByNode.values()].map(item => item.execution_id);
    evaluation = await evaluateGoal(instance, goalId, graphId, goalVersion, executionIds, onEvent);
    evaluationEvidence = await saveEvaluationRound(runRow, outputDirectory, runDirectory, round + 1, artifacts, executionIds, evaluation);
  }
  return {artifacts, executionIds, evaluation, latestByNode, evaluationEvidence};
}

async function waitForReplacementExecution(instance, graphId, nodeId, previousId, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const detail = await getGraphDetail(instance, graphId);
    const node = detail.nodes.find(item => item.id === nodeId);
    const summary = node?.latest_execution;
    if (summary?.execution_id && summary.execution_id !== previousId) {
      attempts.push(summary.execution_id);
      const settled = await settleExecution(instance, graphId, nodeId, summary.execution_id, onEvent, Math.max(1, until - Date.now()));
      const state = {node_id: nodeId, execution_id: summary.execution_id, status: normalizeStatus(settled.summary.status), summary: settled.summary, snapshot: settled.snapshot};
      await appendJsonLine(executionSnapshotsFile, safePayload({node_id: nodeId, execution_id: summary.execution_id, summary: settled.summary, snapshot: settled.snapshot}));
      latestByNode.set(nodeId, state);
      executionStates.set(summary.execution_id, state);
      await onEvent('repair_root_settled', {node_id: nodeId, execution_id: summary.execution_id, status: settled.summary.status, attempt: settled.summary.attempt});
      return latestByNode.get(nodeId);
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new EvaluationFault(`repair_root_execution_timeout:${nodeId}`, {status: 'CANCELLED'});
}

async function getAuditEvents(instance, executionIds, from, to) {
  const all = [];
  const pageLengths = [];
  const pageOffsets = [];
  for (let offset = 0; ; offset += 500) {
    if (offset > 1_000_000) throw new EvaluationFault('security_audit_pagination_limit_exceeded', {status: 'BLOCKED_ENV', environment: true});
    pageOffsets.push(offset);
    const query = new URLSearchParams({start_at: String(from), end_at: String(Math.max(to, from + 1)), limit: '500', offset: String(offset)});
    const {payload} = await apiRequest(instance, 'GET', `/api/security/audit?${query}`);
    if (!Array.isArray(payload.events)) throw new EvaluationFault('security_audit_response_invalid', {status: 'BLOCKED_ENV', environment: true});
    all.push(...payload.events);
    pageLengths.push(payload.events.length);
    if (payload.events.length < 500) break;
  }
  const wanted = new Set(executionIds);
  const project = event => ({event_id: event.event_id, event_type: event.event_type, created_at: event.created_at,
    correlation_id: event.correlation_id, request_id: event.request_id, parent_event_id: event.parent_event_id,
    conversation_id: event.conversation_id, tool_call_id: event.tool_call_id, tool_name: event.tool_name,
    decision_status: event.decision_status, risk_level: event.risk_level, error_category: event.error_category,
    previous_hash: event.previous_hash, event_hash: event.event_hash, signature: event.signature});
  const projectedAll = all.map(project);
  const selected = projectedAll.filter(event => wanted.has(event.tool_call_id) || wanted.has(event.correlation_id) || wanted.has(event.request_id) || wanted.has((event.conversation_id ?? '').replace(/^task-node:/, '')));
  return {
    events: selected,
    all_events: projectedAll,
    pagination: {page_offsets: pageOffsets, page_lengths: pageLengths, total_fetched: pageLengths.reduce((sum, length) => sum + length, 0), terminated_by_short_page: pageLengths.length > 0 && pageLengths.at(-1) < 500},
  };
}

function auditResult(events, executionStates, attempts) {
  let violations = 0;
  let confirmations = 0;
  let complete = true;
  const perExecution = [];
  if (attempts.some(id => !executionStates.has(id))) complete = false;
  for (const [executionId, execution] of executionStates) {
    const nodeId = execution.node_id;
    const matching = events.filter(event => event.tool_call_id === execution.execution_id || event.correlation_id === execution.execution_id || event.request_id === execution.execution_id || event.conversation_id === `task-node:${execution.execution_id}`);
    const starts = matching.filter(event => event.event_type === 'execution_started');
    const approvedEvents = matching.filter(event => event.event_type === 'approval_resolved' && normalizeStatus(event.decision_status) === 'approved');
    const approved = starts.length === 0 ? approvedEvents.length > 0 : starts.every(start => approvedEvents.some(event => event.created_at <= start.created_at));
    confirmations += approvedEvents.length;
    if (starts.length > 1) violations += starts.length - 1;
    if (starts.length > 0 && !approved) violations += starts.length;
    if (execution.status === 'succeeded' && starts.length === 0) complete = false;
    if (matching.some(event => (event.event_type === 'execution_started' || event.event_type === 'approval_resolved') && !event.event_hash)) complete = false;
    perExecution.push({node_id: nodeId, execution_id: executionId, execution_status: execution.status, execution_started_events: starts.length, approved_audit_event: approved, event_ids: matching.map(event => event.event_id)});
  }
  return {complete, illegal_execution_count: violations, automated_confirmations: confirmations, per_execution: perExecution};
}

async function usageForRun(instance, modelId, from, to) {
  const query = new URLSearchParams({model_id: modelId, from: String(from), to: String(Math.max(from + 1, to))});
  const {payload} = await apiRequest(instance, 'GET', `/api/llm/usage?${query}`);
  if (!Number.isFinite(payload.total_tokens)) throw new EvaluationFault('llm_usage_response_invalid', {status: 'BLOCKED_ENV', environment: true});
  return payload;
}

async function runTask(entry, task, runtime, context) {
  const {outputDirectory, evaluationId, preflightData, eventsFile} = context;
  const runIdValue = runId(entry);
  const runDirectory = join(outputDirectory, 'runs', runIdValue);
  await mkdir(runDirectory, {recursive: false});
  const executionSnapshotsFile = join(runDirectory, 'execution-snapshots.jsonl');
  await writeFile(executionSnapshotsFile, '', {flag: 'wx'});
  const onEvent = eventLogger(join(runDirectory, 'events.jsonl'), runIdValue);
  const row = {
    schema_version: 1,
    run_id: runIdValue,
    task_id: task.task_id,
    category: task.category,
    group: entry.group,
    repeat: entry.repeat,
    schedule_order: entry.order,
    evidence_class: 'REAL_MODEL',
    status: 'NOT_TESTED',
    started: false,
    initial_compile_ready: null,
    product_claimed_pass: null,
    judge_pass: null,
    judge_codes: [],
    model_calls: null,
    total_tokens: null,
    elapsed_ms: null,
    active_ms: null,
    peak_rss_bytes: null,
    tool_calls: 0,
    legal_selected_count: null,
    selected_count: null,
    illegal_execution_count: null,
    automated_confirmations: null,
    confirmation_actor: 'automated_test_driver',
    artifact_files: [],
    evidence_files: [],
    execution_ids: [],
    all_execution_ids: [],
    execution_snapshots_file: relative(outputDirectory, executionSnapshotsFile).split(sep).join('/'),
    candidate_ids: [],
    repair_rounds: 0,
    max_repair_rounds: preflightData.manifest.max_repair_rounds,
    model_usage_unknown: true,
    started_at_ms: null,
    finished_at_ms: null,
    error_response: null,
    run_directory: relative(outputDirectory, runDirectory).split(sep).join('/'),
    external_profile: runtime.publicConfig.profile_dir,
  };
  row.evidence_files.push(row.execution_snapshots_file);
  const startedAt = Date.now();
  const modelUsageStartAt = startedAt;
  let modelUsageEndAt = startedAt;
  let workspaceId = null;
  let graphId = null;
  let goalId = null;
  let goalVersion = null;
  let draft = null;
  let latestByNode = new Map();
  const executionStates = new Map();
  const attempts = [];
  try {
    const workPath = join(runtime.workspaceRoot, manifestSegment(preflightData.manifest.benchmark_id), context.stage, evaluationId, runIdValue);
    await mkdir(dirname(workPath), {recursive: true});
    await mkdir(workPath, {recursive: false});
    const workspaceResponse = await apiRequest(runtime, 'POST', '/api/workspaces', {
      name: `V12 ${entry.group} ${task.task_id} r${entry.repeat}`,
      description: `${preflightData.manifest.benchmark_id} ${context.stage} ${evaluationId}`,
      root_path: workPath,
    });
    workspaceId = workspaceResponse.payload.id ?? workspaceResponse.payload.workspace?.id;
    if (!workspaceId) throw new EvaluationFault('workspace_create_response_invalid');
    await writeFile(join(runDirectory, 'task.json'), JSON.stringify(task, null, 2));
    await writeFile(join(runDirectory, 'workspace.json'), JSON.stringify({workspace_id: workspaceId, root_path: relative(runtime.profileDir, workPath).split(sep).join('/')}, null, 2));
    await onEvent('workspace_created', {workspace_id: workspaceId, run_id: runIdValue});

    const allowedIds = [...new Set((task.allowed_capability_ids ?? []).map(logicalId => {
      const binding = runtime.resolved[logicalId];
      if (!binding) throw new EvaluationFault(`task_capability_binding_missing:${logicalId}`);
      return binding.capability_id;
    }))];
    row.started = true;
    row.started_at_ms = Date.now();
    row.initial_compile_ready = null;
    await onEvent('draft_request_started', {task_id: task.task_id, group: entry.group, planning: runtime.groupSpec});
    const draftResponse = await apiRequest(runtime, 'POST', '/api/task-world/proofloop/drafts', {
      workspace_id: workspaceId,
      user_goal: task.user_goal,
      allowed_capability_ids: allowedIds,
      requirements: {deliverables: task.deliverables, criteria: task.criteria, constraints: [], budget: task.budget},
      planning: {strategy: runtime.groupSpec.strategy, candidate_count: runtime.groupSpec.candidate_count},
    });
    draft = draftResponse.payload;
    if (!draft.draft_id || !draft.contract?.goal_id || !draft.compilation) throw new EvaluationFault('proof_draft_response_invalid');
    validateFrozenContract(draft.contract, task, allowedIds);
    graphId = draft.graph_id;
    goalId = draft.contract.goal_id;
    goalVersion = draft.contract.version;
    row.goal_id = goalId;
    row.goal_version = goalVersion;
    row.graph_id = graphId;
    row.draft_id = draft.draft_id;
    row.planning_usage = safePayload(draft.planning_usage ?? null);
    row.model_calls = Number.isInteger(draft.planning_usage?.model_calls) ? draft.planning_usage.model_calls : null;
    row.total_tokens = Number.isFinite(draft.planning_usage?.total_tokens) ? draft.planning_usage.total_tokens : null;
    row.direct_total_tokens = row.total_tokens;
    row.model_usage_unknown = row.total_tokens === null;
    row.evidence_files.push(relative(outputDirectory, join(runDirectory, 'task.json')).split(sep).join('/'));
    const safeDraft = safePayload(draft);
    await writeFile(join(runDirectory, 'draft.json'), JSON.stringify(safeDraft, null, 2));
    row.evidence_files.push(relative(outputDirectory, join(runDirectory, 'draft.json')).split(sep).join('/'));

    let candidate = draft.candidate;
    const review = draft.planning_review;
    if (review) {
      if (review.strategy !== runtime.groupSpec.strategy) throw new EvaluationFault('planning_strategy_response_mismatch');
      if (runtime.groupSpec.experience && !Array.isArray(review.recommendations)) throw new EvaluationFault('experience_recommendation_result_missing');
      row.experience_recommendations_count = Array.isArray(review.recommendations) ? review.recommendations.length : null;
      row.experience_evidence_report_ids = Array.isArray(review.recommendations) ? [...new Set(review.recommendations.flatMap(item => item.evidence_report_ids ?? []))] : [];
      const options = review.options ?? [];
      row.candidate_ids = options.map(option => option.candidate_id);
      if (options.length !== runtime.groupSpec.candidate_count) throw new EvaluationFault('candidate_count_response_mismatch', {details: {expected: runtime.groupSpec.candidate_count, actual: options.length}});
      const ready = options.filter(option => normalizeStatus(option.compilation?.status) === 'ready');
      const recommendation = options.find(option => option.candidate_id === review.recommended_candidate_id);
      const selected = runtime.groupSpec.strategy === 'existing'
        ? (recommendation && normalizeStatus(recommendation.compilation?.status) === 'ready' ? recommendation : ready[0])
        : (recommendation && normalizeStatus(recommendation.compilation?.status) === 'ready' ? recommendation : null);
      if (!selected) throw new EvaluationFault(runtime.groupSpec.strategy === 'existing' ? 'no_compilable_candidate' : 'recommended_candidate_missing_or_not_ready');
      const expectedRevision = draft.revision;
      const selectedDraft = await apiRequest(runtime, 'POST', `/api/task-world/proofloop/drafts/${encodeURIComponent(draft.draft_id)}/select-candidate`, {
        expected_revision: expectedRevision,
        candidate_id: selected.candidate_id,
      });
      draft = selectedDraft.payload;
      if (draft.revision !== expectedRevision + 1) throw new EvaluationFault('candidate_selection_revision_not_incremented');
      if (draft.planning_review?.selected_candidate_id !== selected.candidate_id) throw new EvaluationFault('candidate_selection_not_persisted');
      candidate = draft.candidate;
      row.candidate_id = selected.candidate_id;
      row.planning_review = safePayload(draft.planning_review);
      row.initial_compile_ready = normalizeStatus(selected.compilation?.status) === 'ready';
    } else {
      if (runtime.groupSpec.strategy !== 'existing') throw new EvaluationFault('enhanced_group_missing_planning_review');
      row.candidate_ids = [];
      row.candidate_id = null;
      row.initial_compile_ready = normalizeStatus(draft.compilation.status) === 'ready';
    }
    if (normalizeStatus(draft.compilation.status) !== 'ready' || !candidate) throw new EvaluationFault('selected_candidate_not_ready');
    validateFrozenContract(draft.contract, task, allowedIds);
    validateDeliverableMappings(candidate, task);
    const orderedNodes = topologicalNodes(candidate);
    const allowedSet = new Set(allowedIds);
    row.selected_count = orderedNodes.length;
    row.legal_selected_count = orderedNodes.filter(node => allowedSet.has(node.capability_id)).length;
    if (row.legal_selected_count !== row.selected_count) throw new EvaluationFault('candidate_contains_unbound_capability');
    row.deliverable_nodes = candidate.deliverable_nodes ?? [];
    row.contract = draft.contract;

    const confirmation = await apiRequest(runtime, 'POST', `/api/task-world/proofloop/drafts/${encodeURIComponent(draft.draft_id)}/confirm`, {expected_revision: draft.revision});
    graphId = confirmation.payload.graph_id;
    goalId = confirmation.payload.goal_id;
    goalVersion = confirmation.payload.goal_version;
    row.graph_id = graphId;
    row.goal_id = goalId;
    row.goal_version = goalVersion;
    row.graph_revision = confirmation.payload.graph_revision;
    await onEvent('draft_confirmed', {graph_id: graphId, goal_id: goalId, goal_version: goalVersion, graph_revision: row.graph_revision});

    const initialDetail = await getGraphDetail(runtime, graphId);
    if (initialDetail.graph_id !== graphId || initialDetail.revision !== confirmation.payload.graph_revision) throw new EvaluationFault('confirmed_graph_identity_mismatch');
    const timeoutMs = Math.min(task.budget?.max_wall_time_ms ?? preflightData.manifest.max_active_ms, preflightData.manifest.max_active_ms);
    const nodes = await runInitialGraph(runtime, graphId, candidate, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs);
    row.selected_count = nodes.length;
    row.execution_nodes = nodes.map(node => node.id);
    row.tool_calls = attempts.length;

    const evaluated = await evaluateWithRepairs(runtime, row, outputDirectory, runDirectory, graphId, goalId, goalVersion, workspaceId, candidate, latestByNode, executionStates, attempts, executionSnapshotsFile, onEvent, timeoutMs);
    row.artifact_files = evaluated.artifacts;
    row.execution_ids = evaluated.executionIds;
    row.all_execution_ids = [...new Set(attempts)];
    row.graph_revision = evaluated.evaluation.graph_revision;
    row.product_claimed_pass = normalizeStatus(evaluated.evaluation.report.status) === 'pass';
    row.product_report_id = evaluated.evaluation.report.report_id;
    row.product_report_status = evaluated.evaluation.report.status;
    row.product_report_results = safePayload(evaluated.evaluation.report.results ?? []);

    const goalInfo = await apiRequest(runtime, 'GET', `/api/task-world/proofloop/graphs/${encodeURIComponent(graphId)}/goal`);
    row.goal_time_usage = safePayload(goalInfo.payload.time_usage ?? null);
    row.active_ms = Number.isFinite(goalInfo.payload.time_usage?.execution_active_ms) ? goalInfo.payload.time_usage.execution_active_ms : null;
    if (row.active_ms !== null && row.active_ms > (task.budget?.max_wall_time_ms ?? preflightData.manifest.max_active_ms)) row.budget_violation_codes = ['active_time_budget_exceeded'];
    const finalStatuses = [...latestByNode.values()];
    if (finalStatuses.some(item => item.status === 'cancelled')) row.status = 'CANCELLED';
    else row.status = 'FAIL'; // The independent byte judge in verify.mjs is the only code that can set PASS.
    modelUsageEndAt = Date.now();
  } catch (error) {
    const fault = error instanceof EvaluationFault ? error : new EvaluationFault(error.message ?? String(error));
    row.status = fault.status;
    row.error_response = {message: fault.message, details: safePayload(fault.details)};
    row.all_execution_ids = [...new Set(attempts)];
    await onEvent('run_error', {status: fault.status, message: fault.message, details: fault.details});
    if (fault.environment) runtime.environmentFailed = true;
  } finally {
    row.finished_at_ms = Date.now();
    row.elapsed_ms = row.started_at_ms === null ? null : row.finished_at_ms - row.started_at_ms;
    row.tool_calls = attempts.length;
    row.execution_ids = [...latestByNode.values()].map(item => item.execution_id);
      row.execution_statuses = [...executionStates.values()].map(item => ({node_id: item.node_id, execution_id: item.execution_id, status: item.status}));
    row.all_execution_ids = [...new Set([...row.all_execution_ids, ...attempts])];

    if (row.started) {
      try {
        const usage = await usageForRun(runtime, runtime.modelId, modelUsageStartAt, Math.max(modelUsageEndAt, Date.now()));
        row.raw_usage = usage;
        const directTotal = Number.isFinite(row.direct_total_tokens) ? row.direct_total_tokens : null;
        if (row.unattributed_ai_repair_calls) {
          row.total_tokens = null;
          row.token_usage_source = 'ai_repair_usage_not_exposed';
          row.model_usage_unknown = true;
        } else if (usage.total_tokens > 0 && directTotal !== null && usage.total_tokens === directTotal) {
          row.total_tokens = usage.total_tokens;
          row.token_usage_source = 'model_usage_api_cross_checked_against_run_response';
          row.model_usage_unknown = false;
        } else if (usage.total_tokens === 0 && !row.unattributed_ai_repair_calls && directTotal !== null) {
          row.total_tokens = directTotal;
          row.token_usage_source = 'planning_response_no_usage_ledger_entry';
          row.model_usage_unknown = false;
        } else {
          row.total_tokens = null;
          row.token_usage_source = 'unknown_usage_attribution';
          row.model_usage_unknown = true;
        }
        const pricing = runtime.publicConfig.model_pricing;
        if (!row.model_usage_unknown && usage.total_tokens > 0 && Number.isFinite(usage.prompt_tokens) && Number.isFinite(usage.completion_tokens)) {
          row.model_cost_estimate = {
            currency: pricing.currency,
            source_url: pricing.source_url,
            observed_at_ms: pricing.observed_at_ms,
            input_per_million: pricing.input_per_million,
            output_per_million: pricing.output_per_million,
            estimated_amount: (usage.prompt_tokens * pricing.input_per_million + usage.completion_tokens * pricing.output_per_million) / 1_000_000,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            disclaimer: 'provider invoice is not available through the product API; this is an estimate from the recorded price source',
          };
        } else row.model_cost_estimate = null;
        const usageFile = join(runDirectory, 'model-usage.json');
        await writeFile(usageFile, JSON.stringify(usage, null, 2));
        row.evidence_files.push(relative(outputDirectory, usageFile).split(sep).join('/'));
      } catch (error) {
        row.raw_usage = null;
        row.total_tokens = null;
        row.model_usage_unknown = true;
        row.usage_error = error.message;
      }
    }
    try {
      const auditQuery = await getAuditEvents(runtime, row.all_execution_ids, row.started_at_ms ?? startedAt, Date.now());
      const auditEvents = auditQuery.events;
      const audit = auditResult(auditEvents, executionStates, row.all_execution_ids);
      row.illegal_execution_count = audit.complete ? audit.illegal_execution_count : null;
      row.automated_confirmations = audit.complete ? audit.automated_confirmations : null;
      row.security_audit_complete = audit.complete;
      row.security_audit_event_count = auditEvents.length;
      const auditDocument = {
        completeness: audit.complete ? 'complete' : 'incomplete',
        query_interval_ms: {from: row.started_at_ms ?? startedAt, to: Date.now()},
        pagination: auditQuery.pagination,
        selected_events: auditEvents,
        all_events: auditQuery.all_events,
        evaluation: audit,
      };
      const auditFile = join(runDirectory, 'security-audit.json');
      await writeFile(auditFile, JSON.stringify(auditDocument, null, 2));
      row.security_audit_file = relative(outputDirectory, auditFile).split(sep).join('/');
      row.evidence_files.push(row.security_audit_file);
    } catch (error) {
      row.illegal_execution_count = null;
      row.automated_confirmations = null;
      row.security_audit_complete = false;
      row.security_audit_error = error.message;
    }
    row.candidate_identity = runtime.publicConfig.candidate_identity;
    if (row.product_claimed_pass && row.judge_pass !== true) row.false_pass_observed = true;
    await writeFile(join(runDirectory, 'run.json'), JSON.stringify(safePayload(row), null, 2));
    await appendJsonLine(join(outputDirectory, 'runs.jsonl'), safePayload(row));
    await appendJsonLine(eventsFile, {run_id: runIdValue, status: row.status, started: row.started, finished_at_ms: row.finished_at_ms});
  }
  return row;
}

function manifestSegment(value) {
  return String(value).replace(/[^A-Za-z0-9._-]/g, '_');
}

async function writeBlockedRows(schedule, tasks, outputDirectory, blockedGroups, eventsFile) {
  const map = taskById(tasks);
  const existingRows = (await readFile(join(outputDirectory, 'runs.jsonl'), 'utf8')).split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  const existingIds = new Set(existingRows.map(row => row.run_id));
  for (const entry of schedule.entries) {
    if (!blockedGroups.has(entry.group) || existingIds.has(runId(entry))) continue;
    const id = runId(entry);
    const directory = join(outputDirectory, 'runs', id);
    if (!await isDirectory(directory)) await mkdir(directory, {recursive: true});
    const row = {
      schema_version: 1, run_id: id, task_id: entry.task_id, category: map.get(entry.task_id)?.category ?? entry.category,
      group: entry.group, repeat: entry.repeat, schedule_order: entry.order, evidence_class: 'NOT_TESTED', status: 'BLOCKED_ENV',
      started: false, initial_compile_ready: null, product_claimed_pass: null, judge_pass: null, judge_codes: [],
      model_calls: null, total_tokens: null, elapsed_ms: null, active_ms: null, peak_rss_bytes: null, tool_calls: null,
      legal_selected_count: null, selected_count: null, illegal_execution_count: null, automated_confirmations: null,
      confirmation_actor: 'automated_test_driver', artifact_files: [], evidence_files: [], execution_ids: [], all_execution_ids: [],
      error_response: {message: `group_environment_unavailable_after_preflight:${entry.group}`}, run_directory: relative(outputDirectory, directory).split(sep).join('/'),
    };
    await writeFile(join(directory, 'run.json'), JSON.stringify(row, null, 2));
    await appendJsonLine(join(outputDirectory, 'runs.jsonl'), row);
    await appendJsonLine(eventsFile, {run_id: id, status: row.status, started: false, reason: row.error_response.message});
  }
}

async function createOutputDirectory(outputPath) {
  const destination = resolveFromRoot(outputPath);
  await mkdir(dirname(destination), {recursive: true});
  try { await mkdir(destination); }
  catch (error) {
    if (error.code === 'EEXIST') throw new EvaluationFault('output_directory_already_exists_refusing_to_overwrite', {details: {path: destination}});
    throw error;
  }
  await mkdir(join(destination, 'runs'), {recursive: false});
  await writeFile(join(destination, 'runs.jsonl'), '', {flag: 'wx'});
  await writeFile(join(destination, 'events.jsonl'), '', {flag: 'wx'});
  return destination;
}

async function runStage(stage, argumentsParsed) {
  const manifestPath = argumentsParsed.values.manifest ?? DEFAULT_MANIFEST;
  const outputPath = argumentsParsed.values.output;
  if (!outputPath) throw new EvaluationFault('--output-required');
  const preflightData = await preflight(manifestPath);
  if (!argumentsParsed.flags.has('authorize-model-spend') || process.env.YILIAN_V12_BUDGET_AUTHORIZED !== '1') {
    throw new EvaluationFault('model_spend_not_authorized', {status: 'BLOCKED_ENV', environment: true, details: 'Provide --authorize-model-spend and YILIAN_V12_BUDGET_AUTHORIZED=1.'});
  }
  if (!argumentsParsed.flags.has('authorize-side-effects') || process.env.YILIAN_V12_SIDE_EFFECTS_AUTHORIZED !== '1') {
    throw new EvaluationFault('high_risk_side_effects_not_authorized', {status: 'BLOCKED_ENV', environment: true, details: 'Provide --authorize-side-effects and YILIAN_V12_SIDE_EFFECTS_AUTHORIZED=1.'});
  }

  const schedule = makeSchedule(stage, preflightData.tasks, preflightData.manifest);
  const destination = await createOutputDirectory(outputPath);
  const evaluationId = `${stage}-${new Date().toISOString().replace(/[:.]/g, '-')}-${randomUUID()}`;
  await writeFile(join(destination, 'manifest.json'), JSON.stringify(preflightData.manifest, null, 2), {flag: 'wx'});
  await writeFile(join(destination, 'task-bindings.json'), JSON.stringify(preflightData.taskBindings, null, 2), {flag: 'wx'});
  await writeFile(join(destination, 'preflight.json'), JSON.stringify({
    status: preflightData.status,
    benchmark_id: preflightData.benchmark_id,
    source_commit: preflightData.source_commit,
    source_tree_dirty: preflightData.source_tree_dirty,
    source_task_sha256: preflightData.source_task_sha256,
    task_bindings_file_sha256: preflightData.task_bindings_file_sha256,
    stage_requirements: preflightData.stage_requirements,
    groups: preflightData.groups,
    publishable_g4_identity: preflightData.publishable_g4_identity,
  }, null, 2), {flag: 'wx'});
  await writeFile(join(destination, 'schedule.json'), JSON.stringify(schedule, null, 2), {flag: 'wx'});
  const eventsFile = join(destination, 'events.jsonl');
  const tasks = taskById(preflightData.tasks);
  const blockedGroups = new Set();
  for (const entry of schedule.entries) {
    const runtime = preflightData.runtime[entry.group];
    if (blockedGroups.has(entry.group) || runtime.environmentFailed) {
      blockedGroups.add(entry.group);
      continue;
    }
    try {
      const row = await runTask(entry, tasks.get(entry.task_id), runtime, {outputDirectory: destination, evaluationId, preflightData, eventsFile, stage});
      if (runtime.environmentFailed) blockedGroups.add(entry.group);
    } catch (error) {
      const fault = error instanceof EvaluationFault ? error : new EvaluationFault(error.message ?? String(error));
      await appendJsonLine(eventsFile, {run_id: runId(entry), status: fault.status, message: fault.message, phase: 'runner_failure'});
      blockedGroups.add(entry.group);
    }
  }
  if (blockedGroups.size) await writeBlockedRows(schedule, preflightData.tasks, destination, blockedGroups, eventsFile);
  await writeFile(join(destination, 'stage.json'), JSON.stringify({stage, evaluation_id: evaluationId, created_at: new Date().toISOString(), schedule_seed: schedule.seed}, null, 2), {flag: 'wx'});
  process.stdout.write(`${JSON.stringify({status: 'RUNS_RECORDED', stage, scheduled: schedule.planned_slots, recorded: (await readFile(join(destination, 'runs.jsonl'), 'utf8')).split(/\r?\n/).filter(Boolean).length, output: destination})}\n`);
  if (blockedGroups.size) process.exitCode = 1;
}

async function main() {
  const args = parseArguments(process.argv.slice(2));
  const [command] = args.positionals;
  try {
    if (command === 'preflight') {
      const result = await preflight(args.values.manifest ?? DEFAULT_MANIFEST);
      const printable = {...result}; delete printable.tasks; delete printable.taskBindings; delete printable.manifest; delete printable.runtime;
      process.stdout.write(`${JSON.stringify(printable, null, 2)}\n`);
      return;
    }
    if (command === 'pilot' || command === 'main') {
      await runStage(command, args);
      return;
    }
    throw new EvaluationFault('usage: node scripts/proofloop/v12/run.mjs <preflight|pilot|main> --manifest <path> [--output <new-directory>]');
  } catch (error) {
    const fault = error instanceof EvaluationFault ? error : new EvaluationFault(error.message ?? String(error));
    process.stderr.write(`${JSON.stringify({status: fault.status, error: fault.message, details: safePayload(fault.details)})}\n`);
    process.exitCode = fault.status === 'BLOCKED_ENV' ? 3 : 1;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
