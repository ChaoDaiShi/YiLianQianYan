import { randomUUID } from 'node:crypto';
import { BrowserAdapterError } from './actions.mjs';

const REVISION_INIT_SCRIPT = `(() => {
  const start = () => {
    if (window.__yilianBrowserRevision) return;
    const state = { value: 0 };
    window.__yilianBrowserRevision = state;
    new MutationObserver(() => { state.value += 1; }).observe(document, {
      subtree: true, childList: true, attributes: true, characterData: true
    });
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true });
  else start();
})();`;

function normalizeAllowedOrigins(values) {
  const origins = new Set();
  for (const value of values) {
    const url = new URL(value);
    if (!['http:', 'https:'].includes(url.protocol) || url.origin !== value.replace(/\/$/, '')) {
      throw new Error(`allowlist entries must be HTTP(S) origins without a path: ${value}`);
    }
    origins.add(url.origin);
  }
  return origins;
}

function isAllowedRequest(value, origins) {
  if (['about:', 'data:', 'blob:'].some((protocol) => value.startsWith(protocol))) return true;
  try {
    return origins.has(new URL(value).origin);
  } catch {
    return false;
  }
}

export class BrowserSessions {
  constructor({ chromium, executablePath, allowedOrigins, evidence }) {
    this.chromium = chromium;
    this.executablePath = executablePath;
    this.allowedOrigins = normalizeAllowedOrigins(allowedOrigins);
    this.evidence = evidence;
    this.browser = null;
    this.sessions = new Map();
  }

  async open(urlValue) {
    const url = this.requireAllowedUrl(urlValue);
    if (!this.browser) {
      this.browser = await this.chromium.launch({ executablePath: this.executablePath, headless: true });
    }
    const context = await this.browser.newContext();
    await context.addInitScript(REVISION_INIT_SCRIPT);
    await context.route('**/*', async (route) => {
      if (isAllowedRequest(route.request().url(), this.allowedOrigins)) await route.continue();
      else await route.abort('blockedbyclient');
    });
    const page = await context.newPage();
    const sessionId = randomUUID();
    const pageId = randomUUID();
    const session = { id: sessionId, context, pages: new Map(), evidencePath: null, closing: false };
    const pageState = { id: pageId, page, observations: new Map(), queue: Promise.resolve() };
    session.pages.set(pageId, pageState);
    page.on('popup', (popup) => popup.close().catch(() => {}));
    this.sessions.set(sessionId, session);

    try {
      await page.goto(url.href, { waitUntil: 'domcontentloaded' });
      this.requireAllowedUrl(page.url());
      const title = await page.title();
      const event = await this.record(session, {
        action: 'open', page_id: pageId, url: `${url.origin}${url.pathname}`, outcome: 'opened',
      });
      return { session_id: sessionId, page_id: pageId, url: page.url(), title, ...event };
    } catch (error) {
      this.sessions.delete(sessionId);
      await context.close();
      throw error;
    }
  }

  requireAllowedUrl(value) {
    let url;
    try {
      url = new URL(value);
    } catch {
      throw new BrowserAdapterError('browser_invalid_url', 'url must be an absolute HTTP(S) URL');
    }
    if (!['http:', 'https:'].includes(url.protocol) || !this.allowedOrigins.has(url.origin)) {
      throw new BrowserAdapterError('browser_origin_not_allowed', `origin is outside the invocation allowlist: ${url.origin}`);
    }
    return url;
  }

  getPage(sessionId, pageId) {
    const session = this.sessions.get(sessionId);
    if (!session) throw new BrowserAdapterError('browser_session_not_found', 'browser session is not open');
    if (session.closing) throw new BrowserAdapterError('browser_session_closing', 'browser session is closing');
    const state = session.pages.get(pageId);
    if (!state) throw new BrowserAdapterError('browser_page_not_found', 'page does not belong to this session');
    this.requireAllowedUrl(state.page.url());
    return { session, state };
  }

  async serialized(sessionId, pageId, operation) {
    const { session, state } = this.getPage(sessionId, pageId);
    let release;
    const previous = state.queue;
    state.queue = new Promise((resolve) => { release = resolve; });
    await previous;
    try {
      this.requireAllowedUrl(state.page.url());
      return await operation(session, state);
    } finally {
      release();
    }
  }

  async observe(session, state) {
    this.requireAllowedUrl(state.page.url());
    const snapshot = await state.page.evaluate(() => {
      const text = document.body?.innerText ?? '';
      const nameOf = (element) => {
        const labelledBy = element.getAttribute('aria-labelledby');
        if (labelledBy) {
          const text = labelledBy.split(/\s+/).map((id) => document.getElementById(id)?.innerText ?? '').join(' ').trim();
          if (text) return text;
        }
        const label = element.getAttribute('aria-label');
        if (label) return label.trim();
        if (element.labels?.length) {
          const text = [...element.labels].map((item) => item.innerText).join(' ').trim();
          if (text) return text;
        }
        return (element.getAttribute('alt') || element.getAttribute('title') || element.innerText || element.getAttribute('placeholder') || '').trim().replace(/\s+/g, ' ');
      };
      const implicitRole = (element) => {
        const explicit = element.getAttribute('role');
        if (explicit) return explicit.split(/\s+/)[0];
        const tag = element.tagName.toLowerCase();
        const type = (element.getAttribute('type') || '').toLowerCase();
        if (tag === 'button' || (tag === 'input' && ['button', 'submit', 'reset', 'image'].includes(type))) return 'button';
        if (tag === 'a' && element.hasAttribute('href')) return 'link';
        if (tag === 'textarea' || element.isContentEditable) return 'textbox';
        if (tag === 'input') {
          if (type === 'checkbox') return 'checkbox';
          if (type === 'radio') return 'radio';
          if (['button', 'submit', 'reset', 'image'].includes(type)) return 'button';
          if (type === 'range') return 'slider';
          if (type === 'hidden') return null;
          return 'textbox';
        }
        if (tag === 'select') return element.multiple ? 'listbox' : 'combobox';
        if (/^h[1-6]$/.test(tag)) return 'heading';
        return null;
      };
      const elements = [...document.querySelectorAll('button, a[href], input, textarea, select, [role], [tabindex], [contenteditable="true"]')]
        .filter((element) => {
          const style = getComputedStyle(element);
          return style.display !== 'none' && style.visibility !== 'hidden' && element.getClientRects().length > 0;
        })
        .map((element) => ({
          role: implicitRole(element),
          name: nameOf(element),
          tag: element.tagName.toLowerCase(),
          type: (element.getAttribute('type') || '').toLowerCase(),
          enabled: !element.disabled && element.getAttribute('aria-disabled') !== 'true',
        }))
        .filter((element) => element.role && element.name);
      return {
        title: document.title,
        text: text.slice(0, 20000),
        url: location.href,
        document_id: String(performance.timeOrigin),
        revision: window.__yilianBrowserRevision?.value ?? 0,
        elements,
      };
    });
    const observationId = randomUUID();
    const elementMap = new Map();
    const elements = snapshot.elements.map((element) => {
      const elementId = randomUUID();
      elementMap.set(elementId, element);
      return { element_id: elementId, role: element.role, name: element.name, tag: element.tag, enabled: element.enabled };
    });
    state.observations.set(observationId, {
      documentId: snapshot.document_id,
      revision: snapshot.revision,
      url: snapshot.url,
      elements: elementMap,
    });
    const event = await this.record(session, {
      action: 'read', page_id: state.id, observation_id: observationId,
      url: `${new URL(snapshot.url).origin}${new URL(snapshot.url).pathname}`, outcome: 'observed',
    });
    return { observation_id: observationId, url: snapshot.url, title: snapshot.title, text: snapshot.text, elements, ...event };
  }

  async requireCurrentObservation(state, observationId, elementId) {
    const observation = state.observations.get(observationId);
    if (!observation) throw new BrowserAdapterError('browser_observation_unknown', 'read the page and use an observation from this session');
    const current = await state.page.evaluate(() => ({
      documentId: String(performance.timeOrigin),
      revision: window.__yilianBrowserRevision?.value ?? 0,
      url: location.href,
    }));
    if (current.documentId !== observation.documentId || current.revision !== observation.revision || current.url !== observation.url) {
      throw new BrowserAdapterError('browser_observation_stale', 'page changed after this observation; read it again before acting');
    }
    const target = observation.elements.get(elementId);
    if (!target) throw new BrowserAdapterError('browser_element_unknown', 'element_id does not belong to this observation');
    return target;
  }

  async record(session, event) {
    const evidence = await this.evidence.record(session.id, { occurred_at: new Date().toISOString(), session_id: session.id, ...event });
    session.evidencePath = evidence.evidence_path;
    return evidence;
  }

  async close(sessionId) {
    const session = this.sessions.get(sessionId);
    if (!session) throw new BrowserAdapterError('browser_session_not_found', 'browser session is not open');
    session.closing = true;
    await Promise.all([...session.pages.values()].map((state) => state.queue));
    const event = await this.record(session, { action: 'close', page_ids: [...session.pages.keys()], outcome: 'closed' });
    await session.context.close();
    this.sessions.delete(sessionId);
    if (this.sessions.size === 0) {
      await this.browser?.close();
      this.browser = null;
    }
    return { closed: true, ...event };
  }

  async shutdown() {
    for (const session of this.sessions.values()) await session.context.close().catch(() => {});
    this.sessions.clear();
    await this.browser?.close().catch(() => {});
    this.browser = null;
  }
}
