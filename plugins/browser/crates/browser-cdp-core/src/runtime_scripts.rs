use serde::Deserialize;

use super::PageState;

#[derive(Deserialize)]
pub(super) struct RawPageSnapshot {
    #[serde(flatten)]
    pub(super) page: PageState,
    pub(super) text: String,
    pub(super) text_truncated: bool,
    pub(super) nodes: Vec<RawSnapshotNode>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct RawSnapshotNode {
    pub(super) role: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) value: Option<String>,
    #[serde(default)]
    pub(super) href: Option<String>,
    pub(super) selector: String,
    #[serde(default)]
    pub(super) disabled: bool,
}

pub(super) const PAGE_STATE_SCRIPT: &str = "({url: location.href, title: document.title, ready_state: document.readyState, document_id: String(performance.timeOrigin)})";

pub(super) const VIRTUAL_CURSOR_MARKUP: &str = r##"<style>
  :host { color-scheme: light; }
  .halo {
    position:absolute; left:-13px; top:-13px; width:26px; height:26px;
    border-radius:999px;
    background:radial-gradient(circle,rgba(59,130,246,.24) 0%,rgba(59,130,246,.08) 48%,transparent 72%);
    filter:blur(.4px);
  }
  .pointer {
    position:absolute; left:-3px; top:-3px; width:31px; height:38px;
    transform-origin:5px 5px;
    filter:drop-shadow(0 1px 1px rgba(15,23,42,.35)) drop-shadow(0 5px 9px rgba(15,23,42,.28));
    transition:transform 150ms cubic-bezier(.2,.8,.2,1),filter 150ms ease;
  }
  .pointer.pressed {
    transform:scale(.9) rotate(-2deg);
    filter:drop-shadow(0 1px 1px rgba(15,23,42,.28)) drop-shadow(0 3px 5px rgba(15,23,42,.24));
  }
  .pulse {
    position:absolute; left:-18px; top:-18px; width:36px; height:36px;
    border:2px solid rgba(37,99,235,.9); border-radius:999px;
    opacity:0; transform:scale(.28);
    box-shadow:0 0 0 5px rgba(255,255,255,.82),0 0 18px rgba(37,99,235,.34);
  }
  .pulse::after {
    content:''; position:absolute; inset:6px; border-radius:inherit;
    background:rgba(59,130,246,.2);
  }
  .pulse.active { animation:chatos-click 620ms cubic-bezier(.16,1,.3,1); }
  @keyframes chatos-click {
    0% { opacity:.98; transform:scale(.28); }
    55% { opacity:.5; }
    100% { opacity:0; transform:scale(1.55); }
  }
</style>
<span class="halo"></span>
<svg class="pointer" viewBox="0 0 31 38" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
  <defs>
    <linearGradient id="chatos-cursor-fill" x1="4" y1="3" x2="23" y2="31" gradientUnits="userSpaceOnUse">
      <stop stop-color="#60A5FA"/>
      <stop offset=".48" stop-color="#2563EB"/>
      <stop offset="1" stop-color="#1D4ED8"/>
    </linearGradient>
  </defs>
  <path d="M3.2 2.8v25.4l6.55-5.76 4.72 11.07 5.14-2.2-4.67-10.82h9.92L3.2 2.8Z" fill="url(#chatos-cursor-fill)" stroke="white" stroke-width="2.4" stroke-linejoin="round"/>
  <path d="m9.75 22.44 4.72 11.07 2.57-1.1-4.73-10.98" fill="rgba(15,23,42,.18)"/>
</svg>
<span class="pulse"></span>"##;

pub(super) const SNAPSHOT_SCRIPT: &str = r#"(() => {
  const selectorFor = (el) => {
    if (el.id && CSS.escape) return `#${CSS.escape(el.id)}`;
    const parts = [];
    let node = el;
    while (node && node.nodeType === Node.ELEMENT_NODE && node !== document.body) {
      let part = node.tagName.toLowerCase();
      const siblings = node.parentElement ? [...node.parentElement.children].filter(x => x.tagName === node.tagName) : [];
      if (siblings.length > 1) part += `:nth-of-type(${siblings.indexOf(node) + 1})`;
      parts.unshift(part);
      node = node.parentElement;
    }
    return `body > ${parts.join(' > ')}`;
  };
  const roleFor = (el) => el.getAttribute('role') || ({A:'link',BUTTON:'button',INPUT:'textbox',TEXTAREA:'textbox',SELECT:'combobox',IMG:'img',H1:'heading',H2:'heading',H3:'heading'}[el.tagName] || 'generic');
  const safeHref = (el) => {
    if (el.tagName !== 'A' || !el.hasAttribute('href')) return null;
    try {
      const url = new URL(el.href, location.href);
      if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) return null;
      return url.href;
    } catch { return null; }
  };
  const nodes = [...document.querySelectorAll('a,button,input,textarea,select,[role],[contenteditable="true"],h1,h2,h3,p,time,li')]
    .filter(el => {
      const style = getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    })
    .slice(0, 500)
    .map(el => ({
      role: roleFor(el),
      name: el.getAttribute('aria-label') || el.getAttribute('alt') || el.getAttribute('title') || (el.innerText || el.value || '').trim().slice(0, 300),
      value: typeof el.value === 'string' ? el.value.slice(0, 300) : null,
      href: safeHref(el),
      tag: el.tagName.toLowerCase(),
      selector: selectorFor(el),
      disabled: !!el.disabled
    }));
  const text = (document.querySelector('main,article') || document.body)?.innerText || '';
  return {url: location.href, title: document.title, ready_state: document.readyState,
    document_id: String(performance.timeOrigin), text: text.slice(0, 20000), text_truncated: text.length > 20000, nodes};
})()"#;
