// Keep legacy values for rollback. A present new key always wins, even if invalid.
export function preference(name: 'language' | 'appearance'): string | null {
  try {
    const current = localStorage.getItem(`hopkit-${name}`)
    if (current !== null) return current
    const legacy = localStorage.getItem(`netbox-${name}`)
    const valid = name === 'language' ? ['zh-CN', 'en-US'] : ['fresh', 'readable']
    if (legacy !== null && valid.includes(legacy)) {
      try { localStorage.setItem(`hopkit-${name}`, legacy) } catch { /* Use it this session. */ }
      return legacy
    }
  } catch { /* Preferences are optional when storage is unavailable. */ }
  return null
}

export function savePreference(name: 'language' | 'appearance', value: string): void {
  try { localStorage.setItem(`hopkit-${name}`, value) } catch { /* Keep controls usable. */ }
}
