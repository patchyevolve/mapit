import type { WsEvent } from "./types";

type Listener = (event: WsEvent) => void;

let ws: WebSocket | null = null;
const listeners = new Set<Listener>();

function wsUrl(): string {
  const loc = window.location;
  const proto = loc.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${loc.host}/api/events`;
}

export function connectWs() {
  if (ws && ws.readyState !== WebSocket.CLOSED) return;

  ws = new WebSocket(wsUrl());

  ws.onopen = () => {
    console.log("[ws] connected");
  };

  ws.onmessage = (msg) => {
    try {
      const event: WsEvent = JSON.parse(msg.data);
      listeners.forEach((fn) => fn(event));
    } catch (e) {
      console.warn("[ws] bad message", e);
    }
  };

  ws.onclose = () => {
    console.log("[ws] disconnected");
    ws = null;
    if (listeners.size > 0) setTimeout(connectWs, 3000);
  };

  ws.onerror = () => {
    ws?.close();
  };
}

export function onWsEvent(fn: Listener): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
    if (listeners.size === 0) disconnectWs();
  };
}

export function disconnectWs() {
  if (ws) {
    ws.close();
    ws = null;
  }
}
