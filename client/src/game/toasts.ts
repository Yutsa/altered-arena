import { useSyncExternalStore } from 'react';

export interface Toast {
  id: number;
  text: string;
  kind: 'error' | 'info';
}

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function toast(text: string, kind: Toast['kind'] = 'error', ms = 4500) {
  const t = { id: nextId++, text, kind };
  toasts = [...toasts.filter((x) => x.text !== text), t].slice(-4);
  emit();
  setTimeout(() => dismissToast(t.id), ms);
}

export function dismissToast(id: number) {
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

export function useToasts(): Toast[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => toasts,
  );
}
