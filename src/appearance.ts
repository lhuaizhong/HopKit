import { ref, watch } from 'vue'
import { preference, savePreference } from './preferences'

type Appearance = 'fresh' | 'readable'

function savedAppearance(): Appearance {
  return preference('appearance') === 'readable' ? 'readable' : 'fresh'
}

export const appearance = ref<Appearance>(savedAppearance())

watch(appearance, value => {
  document.documentElement.dataset.theme = value
  savePreference('appearance', value)
}, { immediate: true, flush: 'sync' })
