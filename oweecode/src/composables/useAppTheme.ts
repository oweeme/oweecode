import { ref } from 'vue'

export type AppTheme = 'dark' | 'grey' | 'white'

const theme = ref<AppTheme>((localStorage.getItem('app_theme') as AppTheme) ?? 'dark')

function applyDom(t: AppTheme) {
  if (t === 'dark') document.documentElement.removeAttribute('data-theme')
  else document.documentElement.setAttribute('data-theme', t)
}
applyDom(theme.value)

export function useAppTheme() {
  function setTheme(t: AppTheme) {
    theme.value = t
    localStorage.setItem('app_theme', t)
    applyDom(t)
  }
  return { theme, setTheme }
}
