<script lang="ts">
  // Phones in the side panel: which one to use, the ones remembered for Wi-Fi, and pairing a
  // new one over Wi-Fi (QR code, pairing code, or from the cable).
  import { onDestroy, tick } from "svelte";
  import { ArrowLeft, Check, CircleAlert, LoaderCircle, ShieldAlert, Trash2, Usb, Wifi, WifiOff } from "@lucide/svelte";
  import * as api from "./api";
  import type { DeviceView, Snapshot } from "./api";
  import { t, hasKey } from "./i18n";
  import Segmented from "./Segmented.svelte";

  let { snap, onback }: { snap: Snapshot; onback: () => void } = $props();

  type Method = "qr" | "code" | "cable";
  let method = $state<Method>("qr");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);

  let qrSvg = $state<string | null>(null);
  let qrExpired = $state(false);
  let code = $state("");
  let pairAddress = $state("");
  let address = $state("");
  let addressOpen = $state(false);
  let addressInput = $state<HTMLInputElement>();

  const usbPhone = $derived(snap.devices.find((d) => d.state === "device" && !d.wifi) ?? null);
  const current = $derived(snap.device?.serial ?? null);
  const remembered = $derived(new Map(snap.wifiPhones.filter((p) => p.serial).map((p) => [p.serial!, p])));
  const offline = $derived(snap.wifiPhones.filter((p) => !p.serial));

  // Errors are a key, some with a detail after a colon ("pairedNotFound:192.168.1.20"); anything
  // else is adb's own message.
  function message(e: unknown): string {
    const m = String(e);
    const key = `wifi.err.${m.split(":", 1)[0]}`;
    return hasKey(key) ? t(key) : t("wifi.err.failed", { message: m });
  }

  // Paired, but the phone does not announce itself (a VPN on it): its Wi-Fi IP is known, so
  // "Connect by address" opens with it and only the port from the phone is left to type.
  async function failed(e: unknown) {
    const m = String(e);
    if (m === "cancelled") return;
    error = message(e);
    const ip = m.startsWith("pairedNotFound:") ? m.slice("pairedNotFound:".length) : "";
    if (!ip) return;
    address = `${ip}:`;
    addressOpen = true;
    await tick();
    addressInput?.focus();
    addressInput?.setSelectionRange(address.length, address.length);
  }

  function phoneName(d: DeviceView) {
    return d.name ?? remembered.get(d.serial)?.name ?? d.model;
  }

  function connectionHint(d: DeviceView) {
    if (d.state === "unauthorized") return t("status.unauthorized");
    if (d.state !== "device") return t("status.offline");
    if (!d.wifi) return t("phones.usb");
    return remembered.get(d.serial)?.plain ? t("phones.wifi.plain") : t("phones.wifi");
  }

  async function run(action: () => Promise<string>) {
    busy = true;
    error = null;
    done = null;
    try {
      const serial = await action();
      const d = snap.devices.find((x) => x.serial === serial);
      done = t("wifi.done", { name: d ? phoneName(d) : serial });
      code = "";
    } catch (e) {
      failed(e);
    } finally {
      busy = false;
    }
  }

  // The QR code waits for the phone for a few minutes; a new one replaces it. Once the wait
  // ends (paired, failed or timed out) the code is used up and "New code" is offered.
  let qrRun = 0;
  async function startQr() {
    const mine = ++qrRun;
    qrExpired = false;
    error = null;
    done = null;
    qrSvg = await api.wifiQrStart();
    try {
      const serial = await api.wifiQrWait();
      if (mine !== qrRun) return;
      const d = snap.devices.find((x) => x.serial === serial);
      done = t("wifi.done", { name: d ? phoneName(d) : serial });
      qrExpired = true;
    } catch (e) {
      const m = String(e);
      // "cancelled": a newer code or another method took over.
      if (mine !== qrRun || m === "cancelled") return;
      if (m !== "qrTimeout") failed(e);
      qrExpired = true;
    }
  }

  function choose(m: Method) {
    method = m;
    error = null;
    done = null;
    if (m === "qr") startQr();
    else {
      api.wifiQrCancel();
      qrSvg = null;
    }
  }

  startQr();
  onDestroy(() => api.wifiQrCancel());
</script>

<div class="panel">
  <header>
    <button class="icon-btn" aria-label={t("app.back")} title={t("app.back")} onclick={onback}><ArrowLeft size={20} /></button>
    <h1>{t("phones.title")}</h1>
  </header>

  {#if snap.devices.length > 0 || offline.length > 0}
    <section class="section">
      <h2>{t("phones.list")}</h2>
      <div class="card list" role="radiogroup" aria-label={t("phones.list")}>
        {#each snap.devices as d (d.serial)}
          {@const wifiPhone = remembered.get(d.serial)}
          <div class="row phone">
            <button
              class="pick"
              role="radio"
              aria-checked={d.serial === current}
              disabled={d.state !== "device"}
              onclick={() => api.selectPhone(d.serial)}>
              {#if d.wifi}<Wifi size={18} />{:else}<Usb size={18} />{/if}
              <span class="row-text">
                <span class="name">{phoneName(d)}</span>
                <span class="row-hint">{connectionHint(d)}</span>
              </span>
              {#if d.serial === current}<Check size={18} class="check" />{/if}
            </button>
            {#if wifiPhone}
              <button
                class="icon-btn small"
                title={t("phones.forget")}
                aria-label={`${t("phones.forget")}: ${phoneName(d)}`}
                onclick={() => api.wifiForget(wifiPhone.id)}><Trash2 size={16} /></button>
            {/if}
          </div>
        {/each}
        {#each offline as p (p.id)}
          <div class="row phone">
            <div class="pick off">
              <WifiOff size={18} />
              <span class="row-text">
                <span class="name">{p.name ?? p.id}</span>
                <span class="row-hint">{t(p.plain ? "phones.offline.plain" : "phones.offline")}</span>
              </span>
            </div>
            <button
              class="icon-btn small"
              title={t("phones.forget")}
              aria-label={`${t("phones.forget")}: ${p.name ?? p.id}`}
              onclick={() => api.wifiForget(p.id)}><Trash2 size={16} /></button>
          </div>
        {/each}
      </div>
      {#if snap.devices.filter((d) => d.state === "device").length > 1}
        <button class="auto" aria-pressed={snap.settings.phone === null} onclick={() => api.selectPhone(null)}>
          {t("phones.auto")}
        </button>
      {/if}
      {#if snap.wifiPhones.length > 0}<p class="row-hint note">{t("phones.forget.note")}</p>{/if}
    </section>
  {/if}

  <section class="section">
    <h2>{t("wifi.add")}</h2>
    <Segmented
      label={t("wifi.add")}
      value={method}
      onchange={choose}
      options={[
        { value: "qr", label: t("wifi.method.qr") },
        { value: "code", label: t("wifi.method.code") },
        { value: "cable", label: t("wifi.method.cable") },
      ]} />

    <div class="card body">
      {#if method === "qr"}
        <p>{t("wifi.qr.steps")}</p>
        {#if qrSvg && !qrExpired}
          <div class="qr" role="img" aria-label={t("wifi.method.qr")}>{@html qrSvg}</div>
          <p class="wait"><LoaderCircle size={16} class="spin" />{t("wifi.qr.waiting")}</p>
        {:else if qrExpired}
          {#if !error && !done}<p class="wait">{t("wifi.err.qrTimeout")}</p>{/if}
          <button class="btn" onclick={startQr}>{t("wifi.qr.new")}</button>
        {/if}
      {:else if method === "code"}
        <p>{t("wifi.code.steps")}</p>
        <form
          onsubmit={(e) => {
            e.preventDefault();
            run(() => api.wifiPairCode(code, pairAddress || null));
          }}>
          <label>
            <span>{t("wifi.code.label")}</span>
            <input
              class="code"
              inputmode="numeric"
              autocomplete="off"
              maxlength="7"
              placeholder="123456"
              bind:value={code} />
          </label>
          <label>
            <span>{t("wifi.code.address")}</span>
            <input autocomplete="off" spellcheck="false" placeholder="192.168.1.20:41234" bind:value={pairAddress} />
            <span class="row-hint">{t("wifi.code.address.hint")}</span>
          </label>
          <button class="btn accent" type="submit" disabled={busy || code.replace(/\D/g, "").length !== 6}>
            {#if busy}<LoaderCircle size={18} class="spin" />{t("wifi.pairing")}{:else}{t("wifi.pair")}{/if}
          </button>
        </form>
      {:else}
        <p>{t("wifi.cable.steps")}</p>
        <p class="warning"><ShieldAlert size={16} />{t("wifi.cable.warning")}</p>
        <button class="btn accent" disabled={busy || !usbPhone} onclick={() => run(api.wifiFromCable)}>
          {#if busy}<LoaderCircle size={18} class="spin" />{/if}
          {t("wifi.cable.go")}
        </button>
        {#if !usbPhone}<p class="row-hint">{t("wifi.err.noUsbPhone")}</p>{/if}
      {/if}

      {#if error}
        <p class="error" role="alert"><CircleAlert size={16} />{error}</p>
      {:else if done}
        <p class="done" role="status"><Check size={16} />{done}</p>
      {/if}
    </div>
  </section>

  <details class="section more" bind:open={addressOpen}>
    <summary>{t("wifi.address.title")}</summary>
    <form
      class="card body"
      onsubmit={(e) => {
        e.preventDefault();
        run(() => api.wifiConnect(address));
      }}>
      <p class="row-hint">{t("wifi.address.hint")}</p>
      <input autocomplete="off" spellcheck="false" placeholder="192.168.1.20:37215" aria-label={t("wifi.address.title")} bind:this={addressInput} bind:value={address} />
      <button class="btn" type="submit" disabled={busy || !address.includes(":")}>{t("wifi.connect")}</button>
    </form>
  </details>

  <section class="section">
    <h2>{t("wifi.tips")}</h2>
    <ul class="tips">
      <li>{t("wifi.tip.network")}</li>
      <li>{t("wifi.tip.always")}</li>
      <li>{t("wifi.tip.guest")}</li>
      <li>{t("wifi.tip.hotspot")}</li>
      <li>{t("wifi.tip.vpn")}</li>
      <li>{t("wifi.tip.firewall")}</li>
    </ul>
  </section>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-inline-start: -6px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    line-height: 1.2;
  }
  .phone {
    padding: 0 6px 0 0;
    gap: 4px;
  }
  .pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    min-height: 48px;
    padding: 4px 8px 4px 12px;
    border: 0;
    border-radius: 7px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition-property: background-color;
    transition-duration: 150ms;
  }
  button.pick:hover:not(:disabled) {
    background: var(--control-hover);
  }
  .pick:disabled,
  .pick.off {
    cursor: default;
    color: var(--text-2);
  }
  .pick .row-text {
    flex: 1;
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .pick :global(.check) {
    color: var(--accent-text);
  }
  .icon-btn.small {
    width: 32px;
    height: 32px;
  }
  .auto {
    align-self: flex-start;
    padding: 2px 4px;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .note {
    margin: 0 2px;
    text-wrap: pretty;
  }
  .auto[aria-pressed="true"] {
    color: var(--text-2);
    cursor: default;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
  }
  .body p {
    margin: 0;
    text-wrap: pretty;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  label > span:first-child {
    font-size: 12px;
    color: var(--text-2);
  }
  input {
    min-height: 32px;
    padding: 0 10px;
    border: 1px solid var(--stroke);
    border-bottom-color: var(--text-3);
    border-radius: 4px;
    background: var(--control);
    color: var(--text);
  }
  input:focus {
    outline: none;
    border-bottom: 2px solid var(--accent);
  }
  input.code {
    font-size: 18px;
    letter-spacing: 0.2em;
    font-variant-numeric: tabular-nums;
  }
  .qr {
    align-self: center;
    width: 200px;
    padding: 8px;
    border-radius: 8px;
    /* Scanners want dark on light, whatever the theme. */
    background: #ffffff;
    color: #000000;
  }
  .qr :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }
  .wait,
  .warning,
  .error,
  .done {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
  }
  .wait {
    color: var(--text-2);
  }
  .warning {
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--warn-soft);
  }
  .error {
    color: var(--error);
  }
  .done {
    color: var(--ok);
  }
  .wait :global(svg),
  .warning :global(svg),
  .error :global(svg),
  .done :global(svg) {
    margin-top: 2px;
  }
  .body :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }
  .more summary {
    margin: 0 2px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .more[open] summary {
    margin-bottom: 8px;
  }
  .tips {
    margin: 0;
    padding-inline-start: 18px;
    font-size: 12px;
    color: var(--text-2);
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-wrap: pretty;
  }
</style>
