import { writable } from 'svelte/store';

// Single page-wide "last updated" moment, shared by every card. Each card sets
// it after a successful fetch; the footer reads it — replacing the per-card
// "Updated …" indicators with one trailer.
export const lastUpdated = writable(null);

export function touch() {
  lastUpdated.set(Date.now());
}
