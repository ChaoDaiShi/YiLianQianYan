import { randomUUID } from 'node:crypto';
import { evidenceUrl } from './evidence.mjs';

export class BrowserAdapterError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'BrowserAdapterError';
    this.code = code;
  }
}

function locatorFor(page, target) {
  return page.getByRole(target.role, { name: target.name, exact: true });
}

async function uniqueLocator(page, target) {
  const locator = locatorFor(page, target);
  const count = await locator.count();
  if (count !== 1) throw new BrowserAdapterError('browser_target_ambiguous', `observed target matched ${count} elements`);
  const tag = await locator.evaluate((element) => element.tagName.toLowerCase());
  if (tag !== target.tag) throw new BrowserAdapterError('browser_target_changed', 'observed target structure changed');
  return locator;
}

export async function browserRead(sessions, input) {
  return sessions.serialized(input.session_id, input.page_id, async (session, state) => sessions.observe(session, state));
}

export async function browserFill(sessions, input) {
  return sessions.serialized(input.session_id, input.page_id, async (session, state) => {
    const target = await sessions.requireCurrentObservation(state, input.observation_id, input.element_id);
    if (target.role !== 'textbox' || !['input', 'textarea'].includes(target.tag)) {
      throw new BrowserAdapterError('browser_fill_target_not_textbox', 'browser_fill requires a visible input or textarea');
    }
    const beforeUrl = state.page.url();
    const operationId = randomUUID();
    try {
      const locator = await uniqueLocator(state.page, target);
      await locator.fill(input.value);
      const valueMatches = (await locator.inputValue()) === input.value;
      if (!valueMatches) throw new BrowserAdapterError('browser_fill_not_observed', 'filled value did not match the page value');
      const next = await sessions.observe(session, state);
      const evidence = await sessions.record(session, {
        action: 'fill', page_id: state.id, observation_id: input.observation_id,
        operation_id: operationId, element_id: input.element_id,
        value_length: input.value.length,
        before_url: evidenceUrl(beforeUrl), after_url: evidenceUrl(state.page.url()), outcome: 'value_observed',
      });
      return { changed: true, value_matches: true, next_observation_id: next.observation_id, operation_id: operationId, ...evidence };
    } catch (error) {
      const evidence = await sessions.record(session, {
        action: 'fill', page_id: state.id, observation_id: input.observation_id,
        operation_id: operationId, element_id: input.element_id,
        value_length: input.value.length, before_url: evidenceUrl(beforeUrl),
        after_url: evidenceUrl(state.page.url()), outcome: 'failed', error_code: error.code ?? 'browser_fill_failed',
      });
      if (error instanceof BrowserAdapterError) error.evidence = evidence;
      throw error;
    }
  });
}

export async function browserClick(sessions, input) {
  return sessions.serialized(input.session_id, input.page_id, async (session, state) => {
    const target = await sessions.requireCurrentObservation(state, input.observation_id, input.element_id);
    const beforeUrl = state.page.url();
    const operationId = randomUUID();
    try {
      const locator = await uniqueLocator(state.page, target);
      await locator.click();
      sessions.requireAllowedUrl(state.page.url());
      const next = await sessions.observe(session, state);
      const evidence = await sessions.record(session, {
        action: 'click', page_id: state.id, observation_id: input.observation_id,
        operation_id: operationId, element_id: input.element_id,
        before_url: evidenceUrl(beforeUrl), after_url: evidenceUrl(state.page.url()), outcome: 'click_observed',
      });
      return {
        clicked: true,
        next_observation_id: next.observation_id,
        navigation_url: state.page.url() === beforeUrl ? null : state.page.url(),
        operation_id: operationId,
        ...evidence,
      };
    } catch (error) {
      const evidence = await sessions.record(session, {
        action: 'click', page_id: state.id, observation_id: input.observation_id,
        operation_id: operationId, element_id: input.element_id,
        before_url: evidenceUrl(beforeUrl), after_url: evidenceUrl(state.page.url()), outcome: 'failed',
        error_code: error.code ?? 'browser_click_failed',
      });
      if (error instanceof BrowserAdapterError) error.evidence = evidence;
      throw error;
    }
  });
}

export async function browserSubmit(sessions, input) {
  return sessions.serialized(input.session_id, input.page_id, async (session, state) => {
    const target = await sessions.requireCurrentObservation(state, input.observation_id, input.element_id);
    const locator = await uniqueLocator(state.page, target);
    const submitTarget = await locator.evaluate((element) => {
      const tag = element.tagName.toLowerCase();
      const type = (element.getAttribute('type') || '').toLowerCase();
      const isSubmit = (tag === 'button' && (!type || type === 'submit')) || (tag === 'input' && type === 'submit');
      return { isSubmit, hasForm: Boolean(element.form) };
    });
    if (!submitTarget.isSubmit || !submitTarget.hasForm) {
      throw new BrowserAdapterError('browser_submit_target_not_submit', 'browser_submit requires a submit control associated with a form');
    }
    const beforeUrl = state.page.url();
    const operationId = randomUUID();
    const initialEvidence = await sessions.record(session, {
      action: 'submit', page_id: state.id, observation_id: input.observation_id,
      operation_id: operationId, element_id: input.element_id,
      before_url: evidenceUrl(beforeUrl), outcome: 'started',
    });
    try {
      await locator.click();
      sessions.requireAllowedUrl(state.page.url());
      const next = await sessions.observe(session, state);
      const evidence = await sessions.record(session, {
        action: 'submit', page_id: state.id, observation_id: input.observation_id,
        operation_id: operationId, element_id: input.element_id,
        before_url: evidenceUrl(beforeUrl), after_url: evidenceUrl(state.page.url()),
        next_observation_id: next.observation_id, outcome: 'submit_control_clicked',
      });
      return {
        submitted: true,
        result_state: 'clicked',
        operation_id: operationId,
        final_url: state.page.url(),
        next_observation_id: next.observation_id,
        ...evidence,
      };
    } catch (error) {
      const evidence = await sessions.record(session, {
        action: 'submit', page_id: state.id, observation_id: input.observation_id,
        operation_id: operationId, element_id: input.element_id,
        before_url: evidenceUrl(beforeUrl), after_url: evidenceUrl(state.page.url()),
        outcome: 'unknown', error_code: error.code ?? 'browser_submit_unknown',
      });
      return {
        submitted: null,
        result_state: 'unknown',
        operation_id: operationId,
        final_url: state.page.url(),
        error_code: error.code ?? 'browser_submit_unknown',
        evidence_id: evidence.evidence_id,
        evidence_path: evidence.evidence_path,
        started_evidence_id: initialEvidence.evidence_id,
      };
    }
  });
}

export async function browserCapture(sessions, evidenceStore, input) {
  return sessions.serialized(input.session_id, input.page_id, async (session, state) => {
    const beforeUrl = state.page.url();
    const bytes = await state.page.screenshot({ type: 'png', fullPage: true });
    const screenshot = await evidenceStore.storeScreenshot(session.id, state.id, bytes);
    const observation = await sessions.observe(session, state);
    const evidence = await sessions.record(session, {
      action: 'capture', page_id: state.id, observation_id: observation.observation_id,
      url: evidenceUrl(state.page.url()), screenshot_sha256: screenshot.sha256, outcome: 'captured',
    });
    return { ...screenshot, observation_id: observation.observation_id, url: beforeUrl, ...evidence };
  });
}
