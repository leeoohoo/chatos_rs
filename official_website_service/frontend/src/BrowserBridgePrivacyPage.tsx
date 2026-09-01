// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect } from 'react';
import { ArrowLeft, ExternalLink, ShieldCheck } from 'lucide-react';
import { BrandMark } from './BrandMark';

const EFFECTIVE_DATE = 'September 1, 2026';

function BrowserBridgePrivacyPage() {
  useEffect(() => {
    const previousTitle = document.title;
    document.title = 'Chatos Browser Bridge Privacy Policy | Okra';
    return () => {
      document.title = previousTitle;
    };
  }, []);

  return (
    <main className="privacy-shell">
      <header className="privacy-header">
        <a className="brand" href="/" aria-label="Okra home">
          <BrandMark />
          <span>Okra</span>
        </a>
        <a className="privacy-home-link" href="/"><ArrowLeft size={16} /> Back to home</a>
      </header>

      <article className="privacy-document">
        <div className="privacy-hero">
          <span className="privacy-icon"><ShieldCheck size={30} /></span>
          <p className="privacy-eyebrow">Chrome Extension Privacy Policy</p>
          <h1>Chatos Browser Bridge Privacy Policy</h1>
          <p className="privacy-lead">
            This policy explains how the Chatos Browser Bridge Chrome extension handles data when a
            user connects explicitly selected browser tabs to the locally installed Chatos Browser
            CDP service.
          </p>
          <div className="privacy-meta"><span>Effective date: {EFFECTIVE_DATE}</span><span>Extension: Chatos Browser Bridge</span></div>
        </div>

        <nav className="privacy-summary" aria-label="Policy summary">
          <strong>At a glance</strong>
          <ul>
            <li>Only tabs explicitly shared by the user, plus tabs created by the active task, can be controlled.</li>
            <li>The extension does not sell data or use it for advertising, profiling, or unrelated analytics.</li>
            <li>The extension communicates with a native program on the same computer through local, authenticated channels.</li>
            <li>The user can revoke a shared tab or disconnect the bridge at any time.</li>
          </ul>
        </nav>

        <section>
          <h2>1. Data the extension handles</h2>
          <p>
            The extension handles data only when needed to provide user-requested browser inspection
            and automation. Depending on the requested task and the selected page, this may include:
          </p>
          <ul>
            <li><strong>Browsing activity and tab metadata:</strong> page URLs, titles, tab identifiers, navigation state, and task-created tab groups.</li>
            <li><strong>Website content:</strong> text, document structure, images, links, page state, and other content visible in or available to an explicitly shared tab.</li>
            <li><strong>User activity and automation state:</strong> information needed to perform or verify user-requested navigation, clicks, typing, scrolling, and other page interactions.</li>
            <li><strong>Network and diagnostic data:</strong> network requests, responses, WebSocket activity, console output, and Chrome DevTools Protocol events requested for inspection or troubleshooting.</li>
            <li><strong>Authentication information that may appear in diagnostic data:</strong> request headers, cookies, tokens, or other credentials may be present when the user explicitly requests network inspection or raw Chrome DevTools Protocol operations.</li>
            <li><strong>Local extension preferences:</strong> a local pairing flag used to remember whether the user chose to reconnect the bridge.</li>
          </ul>
          <p>
            The extension is not designed to independently collect names, postal addresses, health
            information, payment information, precise location, or personal communications. Such
            information may nevertheless be part of website content on a tab the user explicitly
            chooses to share. The extension does not use that information for a separate purpose.
          </p>
        </section>

        <section>
          <h2>2. How data is used</h2>
          <p>Data is used only to provide the extension&apos;s single purpose:</p>
          <ul>
            <li>showing the user which tabs are shared;</li>
            <li>inspecting and operating a shared tab at the user&apos;s request;</li>
            <li>creating and organizing tabs created by the active Chatos task;</li>
            <li>performing network, console, page, and runtime diagnostics requested by the user; and</li>
            <li>maintaining the authenticated local bridge and recovering from a local reconnect.</li>
          </ul>
        </section>

        <section>
          <h2>3. Local processing and AI services</h2>
          <p>
            The extension first sends requested data to the locally installed
            <code> ai.chatos.browser_bridge </code>Native Messaging Host and an authenticated loopback
            connection on <code>127.0.0.1</code>. Transmissions between the extension and that native
            program remain on the user&apos;s computer.
          </p>
          <p>
            When the user asks a Chatos or Okra task to inspect or operate a page, the task may send
            the minimum necessary page data to the AI model or service configured for that task. That
            processing is initiated by the user and is necessary to complete the requested feature.
            The applicable provider&apos;s privacy and retention terms may also apply. The extension does
            not send browsing data to an advertising network or an unrelated data broker.
          </p>
        </section>

        <section>
          <h2>4. Storage and retention</h2>
          <p>
            The extension stores only the local pairing preference in Chrome extension storage. It
            does not maintain an extension-owned browsing history, advertising profile, or analytics
            database. Shared-tab and debugging sessions are kept in memory while needed and are
            cleared when the user revokes access, disconnects the bridge, closes the relevant tab, or
            ends the active task.
          </p>
          <p>
            Data intentionally submitted to a user-configured Chatos or AI service may be retained
            according to that service&apos;s settings and privacy terms. Users should avoid sharing a tab
            or requesting diagnostic capture when it contains data they do not want processed.
          </p>
        </section>

        <section>
          <h2>5. Sharing and transfer</h2>
          <p>We do not sell user data. We do not transfer user data for:</p>
          <ul>
            <li>personalized, retargeted, or interest-based advertising;</li>
            <li>creditworthiness or lending decisions;</li>
            <li>unrelated analytics, profiling, or market research; or</li>
            <li>purposes unrelated to the user-facing Browser Bridge feature.</li>
          </ul>
          <p>
            Data is transferred only when necessary to provide the user-requested feature, to a
            service the user has configured or authorized, when required by applicable law, or when
            necessary to protect users and the service from security threats.
          </p>
        </section>

        <section>
          <h2>6. User control and consent</h2>
          <p>
            Installing the extension does not automatically share every browser tab. The user must
            connect the local bridge and explicitly share a current tab, or request a task that
            creates its own tab. The extension rejects privileged Chrome pages and local URL schemes.
          </p>
          <p>
            Users can revoke an individual shared tab, disconnect the extension, close task-created
            tabs, or uninstall the extension. Disconnecting clears active sessions and stops further
            Browser Bridge processing.
          </p>
        </section>

        <section>
          <h2>7. Security</h2>
          <p>
            The extension uses Chrome Native Messaging for local host discovery and an authenticated
            loopback WebSocket for local communication. It accepts messages only from its own
            extension context, restricts control to explicitly shared or task-created tabs, and does
            not execute remotely hosted extension code.
          </p>
        </section>

        <section className="privacy-limited-use">
          <h2>8. Chrome Web Store Limited Use disclosure</h2>
          <p>
            Chatos Browser Bridge&apos;s use of information received from Chrome APIs will adhere to the
            Chrome Web Store User Data Policy, including the Limited Use requirements. Data obtained
            through Chrome APIs is used only to provide or improve the extension&apos;s disclosed,
            user-facing single purpose. It is not used for advertising, sold to third parties, or
            made available for humans to read except with the user&apos;s affirmative consent for a
            specific support purpose, for security, to comply with law, or after aggregation and
            anonymization for permitted internal operations.
          </p>
        </section>

        <section>
          <h2>9. Changes to this policy</h2>
          <p>
            We may update this policy when the extension&apos;s functionality or legal requirements
            change. The effective date at the top of this page will be updated when a material change
            is published. Data-use disclosures in the Chrome Web Store will be updated consistently.
          </p>
        </section>

        <section>
          <h2>10. Contact</h2>
          <p>
            Questions or privacy requests can be submitted through the public project support tracker.
            Please do not include passwords, tokens, private page contents, or other sensitive
            information in a support request.
          </p>
          <div className="privacy-resource-links">
            <a className="privacy-policy-link" href="https://github.com/leeoohoo/chatos_rs/issues" target="_blank" rel="noreferrer">
              Project support tracker <ExternalLink size={14} />
            </a>
            <a className="privacy-policy-link" href="https://developer.chrome.com/docs/webstore/program-policies/limited-use" target="_blank" rel="noreferrer">
              Chrome Web Store Limited Use policy <ExternalLink size={14} />
            </a>
          </div>
        </section>

        <section className="privacy-cn-summary" lang="zh-CN">
          <p className="privacy-eyebrow">中文摘要</p>
          <h2>Chatos Browser Bridge 隐私说明摘要</h2>
          <p>
            本扩展仅处理用户明确共享的标签页，以及当前任务自行创建的标签页。根据用户请求，处理范围可能包括网页地址、网页内容、页面交互、网络请求、调试信息，以及网络诊断中可能包含的 Cookie、令牌等认证信息。
          </p>
          <p>
            扩展先通过 Native Messaging 和经过认证的 <code>127.0.0.1</code> 本机连接与 Chatos Browser CDP 服务通信。当用户要求 AI 任务检查或操作页面时，完成任务所必需的最少页面数据可能由用户配置的 AI 服务处理。
          </p>
          <p>
            我们不销售用户数据，不将数据用于广告、用户画像、信用评估或与浏览器桥接功能无关的分析。用户可随时撤销标签页共享、断开连接或卸载扩展。
          </p>
        </section>
      </article>

      <footer className="privacy-footer">
        <span>© 2025–2026 Okra</span>
        <a href="/">Home</a>
        <a href="/privacy/browser-bridge" aria-current="page">Privacy</a>
      </footer>
    </main>
  );
}

export default BrowserBridgePrivacyPage;
