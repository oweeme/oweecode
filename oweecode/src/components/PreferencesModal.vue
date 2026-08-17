<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n, type Locale } from '../composables/useI18n'
import { useAppTheme } from '../composables/useAppTheme'
import OllamaSetup from './OllamaSetup.vue'
import PodmanSetup from './PodmanSetup.vue'
import { useAppUpdater } from '../composables/useAppUpdater'
import { getVersion } from '@tauri-apps/api/app'

const emit = defineEmits<{ close: [] }>()
const { t, setLocale, locale } = useI18n()
const { theme, setTheme } = useAppTheme()
const showOllamaSetup = ref(false)
const showPodmanSetup = ref(false)

const { state: updaterState, checkForUpdates } = useAppUpdater()
const appVersion = ref('')
getVersion().then(v => { appVersion.value = v }).catch(() => {})

const LOCALES: { code: Locale; flag: string; label: string }[] = [
  { code: 'en', flag: '🇺🇸', label: 'English' },
  { code: 'es', flag: '🇧🇴', label: 'Español' },
  { code: 'de', flag: '🇩🇪', label: 'Deutsch' },
  { code: 'fr', flag: '🇫🇷', label: 'Français' },
  { code: 'ko', flag: '🇰🇷', label: '한국어' },
  { code: 'zh', flag: '🇨🇳', label: '中文' },
  { code: 'ja', flag: '🇯🇵', label: '日本語' },
  { code: 'ru', flag: '🇷🇺', label: 'Русский' },
]

// Load from localStorage
const fontSize    = ref(parseFloat(localStorage.getItem('editor_font_size') ?? '13.5'))
const tabSize     = ref(parseInt(localStorage.getItem('editor_tab_size') ?? '2'))
const wordWrap    = ref(localStorage.getItem('editor_word_wrap') === 'true')
const vimMode     = ref(localStorage.getItem('editor_vim_mode') === 'true')
const autoSave    = ref(localStorage.getItem('editor_auto_save') === 'true')
const fontFamily  = ref(localStorage.getItem('editor_font_family') ?? 'JetBrains Mono')
const lineHeight  = ref(parseFloat(localStorage.getItem('editor_line_height') ?? '1.6'))
const minimap     = ref(localStorage.getItem('editor_minimap') !== 'false')
const selectedLocale = ref<Locale>(locale.value)

const THEMES = computed<{ code: 'dark' | 'grey' | 'white'; label: string; swatch: string }[]>(() => [
  { code: 'dark',  label: t('themeDark'),  swatch: '#141416' },
  { code: 'grey',  label: t('themeGrey'),  swatch: '#38393e' },
  { code: 'white', label: t('themeLight'), swatch: '#f7f7f9' },
])
const selectedTheme = ref<'dark' | 'grey' | 'white'>(theme.value)

// LSP settings
const LSP_LANGS = [
  { key: 'typescript', label: 'TypeScript / JavaScript', server: 'typescript-language-server', install: 'npm i -g typescript-language-server typescript@5.7.3' },
  { key: 'vue',        label: 'Vue / Quasar',            server: 'vue-language-server',         install: 'npm i -g @vue/language-server@1.8.27' },
  { key: 'go',         label: 'Go',                      server: 'gopls',                       install: 'go install golang.org/x/tools/gopls@latest' },
  { key: 'rust',       label: 'Rust',                    server: 'rust-analyzer',               install: 'rustup component add rust-analyzer' },
  { key: 'php',        label: 'PHP',                     server: 'phpactor',                    install: 'composer global require phpactor/phpactor' },
  { key: 'python',     label: 'Python',                  server: 'pylsp',                       install: 'pip install python-lsp-server' },
]
function lspKey(lang: string) { return `lsp_enabled_${lang}` }
const lspEnabled = ref<Record<string, boolean>>(
  Object.fromEntries(LSP_LANGS.map(l => [l.key, localStorage.getItem(lspKey(l.key)) === 'true']))
)

const FONTS = [
  'JetBrains Mono',
  'Fira Code',
  'Cascadia Code',
  'Source Code Pro',
  'Courier New',
  'Consolas',
]

function save() {
  localStorage.setItem('editor_font_size',    String(fontSize.value))
  localStorage.setItem('editor_tab_size',     String(tabSize.value))
  localStorage.setItem('editor_word_wrap',    String(wordWrap.value))
  localStorage.setItem('editor_vim_mode',     String(vimMode.value))
  localStorage.setItem('editor_auto_save',    String(autoSave.value))
  localStorage.setItem('editor_font_family',  fontFamily.value)
  localStorage.setItem('editor_line_height',  String(lineHeight.value))
  localStorage.setItem('editor_minimap',      String(minimap.value))
  setTheme(selectedTheme.value)
  // Save LSP settings
  for (const l of LSP_LANGS) {
    localStorage.setItem(lspKey(l.key), String(lspEnabled.value[l.key] ?? false))
  }
  // Apply locale
  setLocale(selectedLocale.value)
  // Dispatch custom event so EditorArea can react without reload
  window.dispatchEvent(new CustomEvent('prefs-changed', { detail: {
    fontSize: fontSize.value, tabSize: tabSize.value,
    wordWrap: wordWrap.value, autoSave: autoSave.value, vimMode: vimMode.value,
    fontFamily: fontFamily.value, lineHeight: lineHeight.value,
    lsp: { ...lspEnabled.value },
  }}))
  emit('close')
}

function reset() {
  fontSize.value   = 13.5
  tabSize.value    = 2
  wordWrap.value   = false
  vimMode.value    = false
  autoSave.value   = false
  fontFamily.value = 'JetBrains Mono'
  lineHeight.value = 1.6
  minimap.value    = true
  for (const l of LSP_LANGS) lspEnabled.value[l.key] = false
}
</script>

<template>
  <div class="pref-overlay" @click.self="emit('close')">
    <div class="pref-modal">
      <div class="pref-header">
        <span class="pref-title">{{ t('prefsTitle') }}</span>
        <button class="pref-close" @click="emit('close')">×</button>
      </div>

      <div class="pref-body">
        <!-- Editor section -->
        <div class="pref-section-label">{{ t('prefsSectionEditor') }}</div>

        <div class="pref-row">
          <label>{{ t('prefsFontFamily') }}</label>
          <select v-model="fontFamily" class="pref-select">
            <option v-for="f in FONTS" :key="f" :value="f">{{ f }}</option>
          </select>
        </div>

        <div class="pref-row">
          <label>{{ t('prefsFontSize') }} — <span class="pref-val">{{ fontSize }}px</span></label>
          <input type="range" v-model.number="fontSize" min="9" max="28" step="0.5" class="pref-range" />
          <div class="pref-range-labels"><span>9</span><span>28</span></div>
        </div>

        <div class="pref-row">
          <label>{{ t('prefsLineHeight') }} — <span class="pref-val">{{ lineHeight }}</span></label>
          <input type="range" v-model.number="lineHeight" min="1.2" max="2.4" step="0.1" class="pref-range" />
          <div class="pref-range-labels"><span>1.2</span><span>2.4</span></div>
        </div>

        <div class="pref-row">
          <label>{{ t('prefsTabSize') }}</label>
          <div class="pref-tabs-group">
            <button
              v-for="n in [2, 4, 8]" :key="n"
              class="pref-tab-btn"
              :class="{ active: tabSize === n }"
              @click="tabSize = n"
            >{{ n }} {{ t('tabSpaces') }}</button>
          </div>
        </div>

        <!-- Toggles section -->
        <div class="pref-section-label" style="margin-top:14px">{{ t('prefsSectionBehavior') }}</div>

        <div class="pref-row pref-toggle-row">
          <label>{{ t('prefsWordWrap') }}</label>
          <button class="pref-toggle" :class="{ on: wordWrap }" @click="wordWrap = !wordWrap">
            <span class="pref-toggle-knob" />
          </button>
        </div>

        <div class="pref-row pref-toggle-row">
          <label>{{ t('prefsVimMode') }}</label>
          <button class="pref-toggle" :class="{ on: vimMode }" @click="vimMode = !vimMode">
            <span class="pref-toggle-knob" />
          </button>
        </div>

        <!-- Auto-save card -->
        <div class="pref-autosave-card" :class="{ 'pref-autosave-on': autoSave }">
          <div class="pref-autosave-left">
            <div class="pref-autosave-title">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M19 21H5a2 2 0 01-2-2V5a2 2 0 012-2h11l5 5v11a2 2 0 01-2 2z"/>
                <polyline points="17 21 17 13 7 13 7 21"/>
                <polyline points="7 3 7 8 15 8"/>
              </svg>
              {{ t('prefsAutoSave') }}
            </div>
            <div class="pref-autosave-desc">
              <template v-if="autoSave">
                {{ t('prefsAutoSaveOnDesc') }}
              </template>
              <template v-else>
                {{ t('prefsAutoSaveOffDesc') }}
              </template>
            </div>
          </div>
          <button class="pref-toggle" :class="{ on: autoSave }" @click="autoSave = !autoSave">
            <span class="pref-toggle-knob" />
          </button>
        </div>

        <div class="pref-row pref-toggle-row">
          <label>{{ t('prefsMinimap') }}</label>
          <button class="pref-toggle" :class="{ on: minimap }" @click="minimap = !minimap">
            <span class="pref-toggle-knob" />
          </button>
        </div>

        <!-- Theme selector -->
        <div class="pref-section-label" style="margin-top:14px">{{ t('theme') }}</div>
        <div class="pref-theme-grid">
          <button
            v-for="th in THEMES"
            :key="th.code"
            class="pref-theme-btn"
            :class="{ active: selectedTheme === th.code }"
            @click="selectedTheme = th.code"
          >
            <span class="pref-theme-swatch" :style="{ background: th.swatch }" />
            <span>{{ th.label }}</span>
          </button>
        </div>

        <!-- Language selector -->
        <div class="pref-section-label" style="margin-top:14px">{{ t('prefsLanguage') }}</div>
        <div class="pref-lang-grid">
          <button
            v-for="loc in LOCALES"
            :key="loc.code"
            class="pref-lang-btn"
            :class="{ active: selectedLocale === loc.code }"
            @click="selectedLocale = loc.code"
          >
            <span class="pref-lang-flag">{{ loc.flag }}</span>
            <span class="pref-lang-name">{{ loc.label }}</span>
          </button>
        </div>

        <!-- LSP / IntelliSense section -->
        <div class="pref-section-label" style="margin-top:18px">{{ t('prefsSectionLsp') }}</div>
        <div class="lsp-note" v-html="t('prefsLspNote')" />
        <div v-for="lang in LSP_LANGS" :key="lang.key" class="lsp-row">
          <div class="lsp-info">
            <span class="lsp-label">{{ lang.label }}</span>
            <code class="lsp-cmd">{{ lang.install }}</code>
          </div>
          <button class="pref-toggle" :class="{ on: lspEnabled[lang.key] }" @click="lspEnabled[lang.key] = !lspEnabled[lang.key]">
            <span class="pref-toggle-knob" />
          </button>
        </div>

        <!-- Local AI (Ollama) section -->
        <div class="pref-section-label" style="margin-top:18px">AGENTE IA LOCAL</div>
        <p class="lsp-note">
          El modo Agente y el modelo local corren con <strong>Ollama</strong>, gratis y 100% privado — no viene incluido en la app, cada persona lo instala en su máquina la primera vez que quiere usarlo, sea Windows, macOS o Linux.
        </p>
        <button class="pref-ollama-btn" @click="showOllamaSetup = true">⚙ Instalar Ollama / descargar modelos</button>

        <!-- Podman (containers/pods) section -->
        <div class="pref-section-label" style="margin-top:18px">CONTENEDORES Y PODS</div>
        <p class="lsp-note">
          Crear contenedores y pods necesita <strong>Podman</strong> instalado en tu máquina — tampoco viene incluido en la app.
        </p>
        <button class="pref-ollama-btn" @click="showPodmanSetup = true">⚙ {{ t('installPodmanTitle') }}</button>

        <!-- App updates -->
        <div class="pref-section-label" style="margin-top:18px">{{ t('updatesSectionLabel') }}</div>
        <p class="lsp-note">{{ t('currentVersionLabel') }} {{ appVersion || '…' }}</p>
        <div v-if="updaterState.available" class="lsp-note" style="color:var(--accent)">
          ✓ {{ t('updateAvailable') }} v{{ updaterState.version }}
        </div>
        <div v-else-if="updaterState.error" class="lsp-note" style="color:#f85149">{{ updaterState.error }}</div>
        <button class="pref-ollama-btn" :disabled="updaterState.checking" @click="checkForUpdates">
          {{ updaterState.checking ? t('checkingUpdates') : t('checkForUpdates') }}
        </button>

        <!-- Preview box -->
        <div class="pref-section-label" style="margin-top:18px">{{ t('prefsSectionPreview') }}</div>
        <div
          class="pref-preview"
          :style="{
            fontFamily: `'${fontFamily}', monospace`,
            fontSize: fontSize + 'px',
            lineHeight: lineHeight,
          }"
        >fn main() {<br>&nbsp;&nbsp;println!("Hello, OweeCode!");<br>}</div>
      </div>

      <div class="pref-footer">
        <button class="pref-reset" @click="reset">{{ t('prefsReset') }}</button>
        <button class="pref-save" @click="save">{{ t('prefsApply') }}</button>
      </div>
    </div>

    <OllamaSetup v-if="showOllamaSetup" @close="showOllamaSetup = false" />
    <PodmanSetup v-if="showPodmanSetup" @close="showPodmanSetup = false" />
  </div>
</template>

<style scoped>
.pref-overlay {
  position: fixed; inset: 0; z-index: 99999;
  background: rgba(0,0,0,0.65); backdrop-filter: blur(4px);
  display: flex; align-items: center; justify-content: center;
}
.pref-modal {
  background: var(--bg-panel); border: 1px solid var(--border);
  border-radius: 14px; width: 420px; max-height: 86vh;
  display: flex; flex-direction: column;
  box-shadow: 0 24px 80px rgba(0,0,0,0.8);
  font-family: var(--font-ui);
}
.pref-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px 12px; border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.pref-title { font-size: 14px; font-weight: 700; color: var(--fg-bright); }
.pref-close {
  background: none; border: none; color: var(--fg-muted);
  font-size: 20px; cursor: pointer; width: 28px; height: 28px;
  border-radius: 6px; display: flex; align-items: center; justify-content: center;
}
.pref-close:hover { background: var(--bg-hover); color: var(--fg); }

.pref-body { flex: 1; overflow-y: auto; padding: 14px 20px; }
.pref-body::-webkit-scrollbar { width: 4px; }
.pref-body::-webkit-scrollbar-thumb { background: var(--bg-active); border-radius: 2px; }

.pref-section-label {
  font-size: 10px; font-weight: 700; color: var(--fg-muted);
  letter-spacing: 1px; margin-bottom: 10px; margin-top: 4px;
}
.pref-row { margin-bottom: 14px; }
.pref-row label {
  display: block; font-size: 12px; color: var(--fg-dim); margin-bottom: 6px;
}
.pref-val { color: var(--accent); font-weight: 600; }

.pref-select {
  width: 100%; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg); font-size: 12px;
  padding: 5px 8px; outline: none; font-family: var(--font-ui);
}
.pref-select:focus { border-color: var(--accent); }

.pref-range {
  width: 100%; accent-color: var(--accent);
  height: 4px; cursor: pointer;
}
.pref-range-labels {
  display: flex; justify-content: space-between;
  font-size: 10px; color: var(--fg-muted); margin-top: 3px;
}

.pref-tabs-group { display: flex; gap: 6px; }
.pref-tab-btn {
  flex: 1; background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 6px; color: var(--fg-dim); font-size: 11px;
  padding: 5px; cursor: pointer; transition: all 0.12s;
}
.pref-tab-btn:hover { background: var(--bg-hover); color: var(--fg); }
.pref-tab-btn.active {
  background: rgba(208,208,216,0.15); border-color: var(--accent);
  color: var(--accent); font-weight: 600;
}

.pref-theme-grid { display: flex; gap: 8px; margin-bottom: 6px; }
.pref-theme-btn {
  flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px;
  background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 8px; color: var(--fg-dim); font-size: 11px;
  padding: 8px 6px; cursor: pointer; transition: all 0.12s;
}
.pref-theme-btn:hover { background: var(--bg-hover); color: var(--fg); }
.pref-theme-btn.active { border-color: var(--accent); color: var(--accent); font-weight: 700; }
.pref-theme-swatch {
  width: 100%; height: 28px; border-radius: 5px; border: 1px solid var(--border);
}

.pref-lang-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 6px;
  margin-bottom: 6px;
}
.pref-lang-btn {
  display: flex; flex-direction: column; align-items: center; gap: 3px;
  background: var(--bg-mid); border: 1px solid var(--border);
  border-radius: 8px; color: var(--fg-dim); font-size: 11px;
  padding: 7px 6px; cursor: pointer; transition: all 0.12s;
}
.pref-lang-btn:hover { background: var(--bg-hover); color: var(--fg); }
.pref-lang-btn.active {
  background: rgba(208,208,216,0.15); border-color: var(--accent);
  color: var(--accent); font-weight: 700;
}
.pref-lang-flag { font-size: 18px; line-height: 1; }
.pref-lang-name { font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }

/* Auto-save card */
.pref-autosave-card {
  display: flex; align-items: center; gap: 12px;
  padding: 10px 12px; border-radius: 8px; margin-bottom: 14px;
  background: var(--bg-mid); border: 1px solid var(--border);
  transition: border-color 0.2s, background 0.2s;
}
.pref-autosave-card.pref-autosave-on {
  background: rgba(208,208,216,0.07);
  border-color: rgba(208,208,216,0.3);
}
.pref-autosave-left { flex: 1; min-width: 0; }
.pref-autosave-title {
  display: flex; align-items: center; gap: 6px;
  font-size: 12px; color: var(--fg); font-weight: 600; margin-bottom: 3px;
}
.pref-autosave-desc {
  font-size: 11px; color: var(--fg-muted); line-height: 1.4;
}

.pref-toggle-row { display: flex; align-items: center; justify-content: space-between; }
.pref-toggle-row label { margin: 0; }
.pref-toggle {
  width: 36px; height: 20px; border-radius: 10px;
  background: var(--bg-active); border: none; cursor: pointer;
  position: relative; transition: background 0.2s; flex-shrink: 0;
}
.pref-toggle.on { background: var(--accent); }
.pref-toggle-knob {
  position: absolute; top: 2px; left: 2px;
  width: 16px; height: 16px; border-radius: 50%;
  background: #fff; transition: transform 0.2s;
  display: block;
}
.pref-toggle.on .pref-toggle-knob { transform: translateX(16px); }

.lsp-note {
  font-size: 11px;
  color: var(--fg-muted);
  line-height: 1.5;
  margin-bottom: 10px;
  padding: 8px 10px;
  background: rgba(208,208,216,0.07);
  border: 1px solid rgba(208,208,216,0.2);
  border-radius: 6px;
}
.pref-ollama-btn {
  width: 100%; background: rgba(166,227,161,0.1); border: 1px solid rgba(166,227,161,0.35);
  color: #a6e3a1; border-radius: 6px; padding: 8px 12px; font-size: 12px; font-weight: 600; cursor: pointer;
}
.pref-ollama-btn:hover { background: rgba(166,227,161,0.18); }
.lsp-note :deep(strong) { color: var(--accent); }
.lsp-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 0;
  border-bottom: 1px solid var(--border);
}
.lsp-info { flex: 1; min-width: 0; }
.lsp-label { display: block; font-size: 12px; color: var(--fg); margin-bottom: 2px; }
.lsp-cmd {
  font-size: 10px;
  color: var(--fg-muted);
  font-family: var(--font-mono);
  background: var(--bg-darkest);
  padding: 2px 5px;
  border-radius: 3px;
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pref-preview {
  background: #0d0e10; border: 1px solid var(--border);
  border-radius: 8px; padding: 12px 14px;
  color: #cdd6f4; transition: all 0.2s;
}

.pref-footer {
  display: flex; gap: 8px; padding: 12px 20px;
  border-top: 1px solid var(--border); flex-shrink: 0;
}
.pref-reset {
  background: none; border: 1px solid var(--border); border-radius: 7px;
  color: var(--fg-muted); font-size: 12px; padding: 7px 14px; cursor: pointer;
}
.pref-reset:hover { border-color: var(--fg-muted); color: var(--fg); }
.pref-save {
  flex: 1; background: var(--accent); border: none; border-radius: 7px;
  color: var(--accent-fg); font-size: 12.5px; font-weight: 600;
  padding: 7px; cursor: pointer; transition: opacity 0.12s;
}
.pref-save:hover { opacity: 0.85; }
</style>
