import { ref, watch } from 'vue'

type Appearance = 'fresh' | 'readable'
const storageKey = 'netbox-appearance'

function savedAppearance(): Appearance {
  try {
    return localStorage.getItem(storageKey) === 'readable' ? 'readable' : 'fresh'
  } catch {
    return 'fresh'
  }
}

export const appearance = ref<Appearance>(savedAppearance())

watch(appearance, value => {
  document.documentElement.dataset.theme = value
  try {
    localStorage.setItem(storageKey, value)
  } catch {
    // Keep switching usable when local preferences cannot be saved.
  }
}, { immediate: true, flush: 'sync' })
