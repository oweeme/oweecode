<script setup lang="ts">
import { reactive, ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from '../composables/useI18n'
import { useEditorStore } from '../composables/useEditorStore'

interface PortMapping { host: string; container: string; protocol: string }
interface VolumeMapping { host: string; container: string }
interface EnvVar { key: string; value: string }
interface ContainerOpts {
  name: string
  image: string
  ports: PortMapping[]
  volumes: VolumeMapping[]
  env: EnvVar[]
  command: string
  restart_policy: string
  network: string
  workdir: string
}
interface ImageInfo { id: string; repository: string; tag: string; size: string }

const props = defineProps<{ mode: 'create' | 'edit'; containerId?: string; initial?: ContainerOpts }>()
const emit = defineEmits<{ close: []; saved: [id: string] }>()
const { t } = useI18n()
const editorStore = useEditorStore()
const projectPath = computed(() => editorStore.state.rootPath)

function blankForm(): ContainerOpts {
  return { name: '', image: '', ports: [], volumes: [], env: [], command: '', restart_policy: '', network: '', workdir: '' }
}

const form = reactive<ContainerOpts>(props.initial ? { ...props.initial, ports: [...props.initial.ports], volumes: [...props.initial.volumes], env: [...props.initial.env] } : blankForm())

type TemplateId = 'nginx' | 'node' | 'php' | 'apache-php' | 'python' | 'java' | 'csharp' | 'rust' | 'go' | 'mariadb' | 'postgres'

function projectVolume(containerPath: string): VolumeMapping {
  return { host: projectPath.value || '/ruta/a/tu/proyecto', container: containerPath }
}

const BUILTIN_TEMPLATES: {
  id: TemplateId; label: string; kind: 'backend' | 'database'
  volumeContainerPath?: string // where the current project gets mounted, if applicable
  opts: Omit<ContainerOpts, 'volumes'> & { volumes?: VolumeMapping[] }
}[] = [
  { id: 'nginx', label: 'Nginx', kind: 'backend', opts: {
    name: 'web_nginx', image: 'docker.io/library/nginx:latest',
    ports: [{ host: '8080', container: '80', protocol: 'tcp' }],
    volumes: [{ host: 'nginx_html', container: '/usr/share/nginx/html' }],
    env: [], command: '', restart_policy: 'unless-stopped', network: '', workdir: '',
  } },
  { id: 'node', label: 'Node', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'node_app', image: 'docker.io/library/node:20',
    ports: [{ host: '3000', container: '3000', protocol: 'tcp' }],
    env: [], command: 'npm start', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'php', label: 'PHP 8.4 (dev server)', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'php_app', image: 'docker.io/library/php:8.4-cli',
    ports: [{ host: '8000', container: '8000', protocol: 'tcp' }],
    env: [], command: 'php -S 0.0.0.0:8000 -t /app', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'apache-php', label: 'Apache + PHP 8.4', kind: 'backend', volumeContainerPath: '/var/www/html', opts: {
    name: 'apache_php_app', image: 'docker.io/library/php:8.4-apache',
    ports: [{ host: '8080', container: '80', protocol: 'tcp' }],
    env: [], command: '', restart_policy: 'unless-stopped', network: '', workdir: '',
  } },
  { id: 'python', label: 'Python', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'python_app', image: 'docker.io/library/python:3.12',
    ports: [{ host: '8000', container: '8000', protocol: 'tcp' }],
    env: [], command: 'python3 app.py', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'java', label: 'Java', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'java_app', image: 'docker.io/library/maven:3.9-eclipse-temurin-21',
    ports: [{ host: '8080', container: '8080', protocol: 'tcp' }],
    env: [], command: 'mvn spring-boot:run', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'csharp', label: 'C#', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'csharp_app', image: 'mcr.microsoft.com/dotnet/sdk:8.0',
    ports: [{ host: '8080', container: '8080', protocol: 'tcp' }],
    env: [], command: 'dotnet run --urls=http://0.0.0.0:8080', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'rust', label: 'Rust', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'rust_app', image: 'docker.io/library/rust:1.82',
    ports: [{ host: '8080', container: '8080', protocol: 'tcp' }],
    env: [], command: 'cargo run', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'go', label: 'Go', kind: 'backend', volumeContainerPath: '/app', opts: {
    name: 'go_app', image: 'docker.io/library/golang:1.22',
    ports: [{ host: '8081', container: '8080', protocol: 'tcp' }],
    env: [], command: 'go run .', restart_policy: '', network: '', workdir: '/app',
  } },
  { id: 'mariadb', label: 'MariaDB', kind: 'database', opts: {
    name: 'mariadb_db', image: 'docker.io/library/mariadb:11',
    ports: [{ host: '3306', container: '3306', protocol: 'tcp' }],
    volumes: [{ host: 'mariadb_data', container: '/var/lib/mysql' }],
    env: [{ key: 'MARIADB_ROOT_PASSWORD', value: 'changeme' }, { key: 'MARIADB_DATABASE', value: 'mi_base' }],
    command: '', restart_policy: 'unless-stopped', network: '', workdir: '',
  } },
  { id: 'postgres', label: 'PostgreSQL', kind: 'database', opts: {
    name: 'postgres_db', image: 'docker.io/library/postgres:16',
    ports: [{ host: '5432', container: '5432', protocol: 'tcp' }],
    volumes: [{ host: 'postgres_data', container: '/var/lib/postgresql/data' }],
    env: [{ key: 'POSTGRES_PASSWORD', value: 'changeme' }, { key: 'POSTGRES_DB', value: 'mi_base' }],
    command: '', restart_policy: 'unless-stopped', network: '', workdir: '',
  } },
]

function applyTemplate(id: TemplateId) {
  const tpl = BUILTIN_TEMPLATES.find(x => x.id === id)
  if (!tpl) return
  stackMode.value = ''
  const volumes = tpl.opts.volumes ?? (tpl.volumeContainerPath ? [projectVolume(tpl.volumeContainerPath)] : [])
  Object.assign(form, { ...tpl.opts, volumes, ports: tpl.opts.ports.map(p => ({ ...p })), env: tpl.opts.env.map(e => ({ ...e })) })
}

// ── Nginx + PHP-FPM stack: two containers (nginx as reverse proxy, php-fpm as
// the interpreter) sharing a network, plus a generated nginx.conf bind-mounted
// into the nginx container — nginx alone cannot execute PHP.
const stackMode = ref<'' | 'nginx-php-fpm'>('')
const nginxStackName = ref('nginx_web')
const nginxStackPort = ref('8080')

function applyNginxPhpFpmTemplate() {
  Object.assign(form, {
    name: 'php_fpm', image: 'docker.io/library/php:8.4-fpm',
    ports: [], volumes: [projectVolume('/var/www/html')],
    env: [], command: '', restart_policy: 'unless-stopped', network: '', workdir: '',
  })
  stackMode.value = 'nginx-php-fpm'
  nginxStackName.value = 'nginx_web'
  nginxStackPort.value = '8080'
}

function buildNginxPhpFpmConf(phpContainerName: string): string {
  return `server {
    listen 80;
    root /var/www/html;
    index index.php index.html;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \\.php$ {
        fastcgi_pass ${phpContainerName}:9000;
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        include fastcgi_params;
    }

    location ~ /\\.ht {
        deny all;
    }
}
`
}

// ── custom (user-saved) templates ────────────────────────────────────────────
interface CustomTemplate { id: string; label: string; opts: ContainerOpts }
const CUSTOM_KEY = 'container_custom_templates'
function loadCustomTemplates(): CustomTemplate[] {
  try { return JSON.parse(localStorage.getItem(CUSTOM_KEY) ?? '[]') } catch { return [] }
}
const customTemplates = ref<CustomTemplate[]>(loadCustomTemplates())
function saveCustomTemplates() {
  localStorage.setItem(CUSTOM_KEY, JSON.stringify(customTemplates.value))
}
const newTemplateName = ref('')
const showSaveTemplate = ref(false)
function saveAsTemplate() {
  if (!newTemplateName.value.trim()) return
  customTemplates.value.push({
    id: crypto.randomUUID(), label: newTemplateName.value.trim(),
    opts: { ...form, ports: form.ports.map(p => ({ ...p })), volumes: form.volumes.map(v => ({ ...v })), env: form.env.map(e => ({ ...e })) },
  })
  saveCustomTemplates()
  newTemplateName.value = ''
  showSaveTemplate.value = false
}
function applyCustomTemplate(tpl: CustomTemplate) {
  stackMode.value = ''
  Object.assign(form, { ...tpl.opts, ports: tpl.opts.ports.map(p => ({ ...p })), volumes: tpl.opts.volumes.map(v => ({ ...v })), env: tpl.opts.env.map(e => ({ ...e })) })
}
function removeCustomTemplate(id: string) {
  customTemplates.value = customTemplates.value.filter(t => t.id !== id)
  saveCustomTemplates()
}

// ── link a companion database (shared network) ──────────────────────────────
const linkDb = ref(false)
const linkDbEngine = ref<'mariadb' | 'postgres'>('mariadb')
const linkDbName = ref('app_db')
const linkDbPassword = ref('changeme')
const linkDbPort = ref('3306')
function onLinkDbEngineChange() {
  linkDbPort.value = linkDbEngine.value === 'mariadb' ? '3306' : '5432'
}

const images = ref<ImageInfo[]>([])
const pulling = ref(false)
const pullError = ref('')
const saving = ref(false)
const saveError = ref('')

async function loadImages() {
  try { images.value = await invoke<ImageInfo[]>('image_list') } catch { /* ignore, form still usable */ }
}

async function pullImage() {
  if (!form.image.trim()) return
  pulling.value = true
  pullError.value = ''
  try {
    await invoke('image_pull', { image: form.image.trim() })
    await loadImages()
  } catch (e: any) { pullError.value = String(e) }
  finally { pulling.value = false }
}

function addPort() { form.ports.push({ host: '', container: '', protocol: 'tcp' }) }
function removePort(i: number) { form.ports.splice(i, 1) }
function addVolume() { form.volumes.push({ host: '', container: '' }) }
function removeVolume(i: number) { form.volumes.splice(i, 1) }
function addEnv() { form.env.push({ key: '', value: '' }) }
function removeEnv(i: number) { form.env.splice(i, 1) }

async function submit() {
  if (!form.name.trim() || !form.image.trim()) return
  if (stackMode.value === 'nginx-php-fpm' && !projectPath.value) {
    saveError.value = t('needProjectOpenHint')
    return
  }
  saving.value = true
  saveError.value = ''
  try {
    let networkName = form.network.trim()
    let extraEnv: EnvVar[] = []

    if ((linkDb.value || stackMode.value === 'nginx-php-fpm') && props.mode === 'create') {
      networkName = `${form.name.trim()}_net`
      await invoke('network_ensure', { name: networkName })
    }

    if (linkDb.value && props.mode === 'create') {
      const dbName = `${form.name.trim()}_db`
      const isMaria = linkDbEngine.value === 'mariadb'
      const dbOpts: ContainerOpts = {
        name: dbName,
        image: isMaria ? 'docker.io/library/mariadb:11' : 'docker.io/library/postgres:16',
        ports: [{ host: linkDbPort.value, container: linkDbPort.value, protocol: 'tcp' }],
        volumes: [{ host: `${dbName}_data`, container: isMaria ? '/var/lib/mysql' : '/var/lib/postgresql/data' }],
        env: isMaria
          ? [{ key: 'MARIADB_ROOT_PASSWORD', value: linkDbPassword.value }, { key: 'MARIADB_DATABASE', value: linkDbName.value }]
          : [{ key: 'POSTGRES_PASSWORD', value: linkDbPassword.value }, { key: 'POSTGRES_DB', value: linkDbName.value }],
        command: '', restart_policy: 'unless-stopped', network: networkName, workdir: '',
      }
      await invoke<string>('container_create', { opts: dbOpts })

      extraEnv = [
        { key: 'DB_HOST', value: dbName },
        { key: 'DB_PORT', value: linkDbPort.value },
        { key: 'DB_NAME', value: linkDbName.value },
        { key: 'DB_USER', value: isMaria ? 'root' : 'postgres' },
        { key: 'DB_PASSWORD', value: linkDbPassword.value },
      ]
    }

    const opts = {
      ...form,
      ports: form.ports.filter(p => p.host && p.container),
      volumes: form.volumes.filter(v => v.host && v.container),
      env: [...form.env.filter(e => e.key), ...extraEnv],
      network: networkName,
    }
    const id = props.mode === 'edit' && props.containerId
      ? await invoke<string>('container_update', { id: props.containerId, opts })
      : await invoke<string>('container_create', { opts })

    if (stackMode.value === 'nginx-php-fpm' && props.mode === 'create') {
      const confPath = `${projectPath.value}/.oweemeide/nginx-php-fpm.conf`
      await invoke('write_config_file', { path: confPath, content: buildNginxPhpFpmConf(form.name.trim()) })
      const nginxOpts: ContainerOpts = {
        name: nginxStackName.value.trim() || 'nginx_web',
        image: 'docker.io/library/nginx:latest',
        ports: [{ host: nginxStackPort.value, container: '80', protocol: 'tcp' }],
        volumes: [
          { host: projectPath.value, container: '/var/www/html' },
          { host: confPath, container: '/etc/nginx/conf.d/default.conf' },
        ],
        env: [], command: '', restart_policy: 'unless-stopped', network: networkName, workdir: '',
      }
      await invoke<string>('container_create', { opts: nginxOpts })
    }

    emit('saved', id)
  } catch (e: any) { saveError.value = String(e) }
  finally { saving.value = false }
}

onMounted(loadImages)
</script>

<template>
  <div class="cf-overlay" @click.self="emit('close')">
    <div class="cf-modal">
      <div class="cf-head">
        <span>{{ mode === 'edit' ? t('editContainer') : t('createContainer') }}</span>
        <button class="cf-close" @click="emit('close')">×</button>
      </div>

      <div class="cf-body">
        <template v-if="mode === 'create'">
          <div class="cf-field">
            <label>{{ t('quickTemplatesBackend') }}</label>
            <div class="cf-template-row">
              <button v-for="tpl in BUILTIN_TEMPLATES.filter(x => x.kind === 'backend')" :key="tpl.id"
                class="cf-template-btn" @click="applyTemplate(tpl.id)">{{ tpl.label }}</button>
            </div>
          </div>

          <div class="cf-field">
            <label>{{ t('quickTemplatesDb') }}</label>
            <div class="cf-template-row">
              <button v-for="tpl in BUILTIN_TEMPLATES.filter(x => x.kind === 'database')" :key="tpl.id"
                class="cf-template-btn" @click="applyTemplate(tpl.id)">{{ tpl.label }}</button>
            </div>
          </div>

          <div class="cf-field">
            <label>{{ t('quickTemplatesStacks') }}</label>
            <div class="cf-template-row">
              <button class="cf-template-btn" @click="applyNginxPhpFpmTemplate">Nginx + PHP-FPM</button>
            </div>
          </div>

          <div v-if="stackMode === 'nginx-php-fpm'" class="cf-section cf-linkdb">
            <div class="cf-hint">{{ t('nginxPhpFpmHint') }}</div>
            <div class="cf-row2">
              <div class="cf-field" style="flex:1">
                <label>{{ t('containerNameLabel') }} (Nginx)</label>
                <input v-model="nginxStackName" class="cf-input" />
              </div>
              <div class="cf-field" style="flex:1">
                <label>{{ t('hostPort') }}</label>
                <input v-model="nginxStackPort" class="cf-input" />
              </div>
            </div>
          </div>

          <div v-if="customTemplates.length" class="cf-field">
            <label>{{ t('customTemplates') }}</label>
            <div class="cf-template-row">
              <div v-for="tpl in customTemplates" :key="tpl.id" class="cf-custom-template">
                <button class="cf-template-btn" @click="applyCustomTemplate(tpl)">{{ tpl.label }}</button>
                <button class="cf-custom-template-del" @click="removeCustomTemplate(tpl.id)" :title="t('delete')">×</button>
              </div>
            </div>
          </div>
        </template>

        <div class="cf-field">
          <label>{{ t('containerNameLabel') }}{{ stackMode === 'nginx-php-fpm' ? ' (PHP-FPM)' : '' }}</label>
          <input v-model="form.name" class="cf-input" placeholder="my_container" :disabled="mode === 'edit'" />
        </div>

        <div class="cf-field">
          <label>{{ t('imageLabel') }}</label>
          <div class="cf-image-row">
            <input v-model="form.image" class="cf-input" placeholder="docker.io/library/nginx:latest" list="cf-image-list" />
            <datalist id="cf-image-list">
              <option v-for="img in images" :key="img.id" :value="`${img.repository}:${img.tag}`" />
            </datalist>
            <button class="cf-pull-btn" @click="pullImage" :disabled="pulling || !form.image.trim()">
              <span v-if="pulling" class="cf-spin" />
              <span v-else>{{ t('pullImage') }}</span>
            </button>
          </div>
          <div v-if="pullError" class="cf-inline-error">{{ pullError }}</div>
        </div>

        <div class="cf-section">
          <div class="cf-section-head">
            <span>{{ t('portsLabel') }}</span>
            <button class="cf-add-row-btn" @click="addPort">+ {{ t('addPort') }}</button>
          </div>
          <div v-for="(p, i) in form.ports" :key="i" class="cf-row">
            <input v-model="p.host" class="cf-input cf-input-sm" :placeholder="t('hostPort')" />
            <span class="cf-sep">:</span>
            <input v-model="p.container" class="cf-input cf-input-sm" :placeholder="t('containerPort')" />
            <select v-model="p.protocol" class="cf-input cf-input-xs">
              <option value="tcp">TCP</option>
              <option value="udp">UDP</option>
            </select>
            <button class="cf-row-del" @click="removePort(i)">×</button>
          </div>
        </div>

        <div class="cf-section">
          <div class="cf-section-head">
            <span>{{ t('volumesLabel') }}</span>
            <button class="cf-add-row-btn" @click="addVolume">+ {{ t('addVolume') }}</button>
          </div>
          <div class="cf-hint">{{ t('volumeHint') }}</div>
          <div v-for="(v, i) in form.volumes" :key="i" class="cf-row">
            <input v-model="v.host" class="cf-input" :placeholder="t('hostPath')" />
            <span class="cf-sep">:</span>
            <input v-model="v.container" class="cf-input" :placeholder="t('containerPath')" />
            <button class="cf-row-del" @click="removeVolume(i)">×</button>
          </div>
        </div>

        <div class="cf-section">
          <div class="cf-section-head">
            <span>{{ t('envVarsLabel') }}</span>
            <button class="cf-add-row-btn" @click="addEnv">+ {{ t('addEnvVar') }}</button>
          </div>
          <div v-for="(e, i) in form.env" :key="i" class="cf-row">
            <input v-model="e.key" class="cf-input" placeholder="KEY" />
            <span class="cf-sep">=</span>
            <input v-model="e.value" class="cf-input" :placeholder="t('value')" />
            <button class="cf-row-del" @click="removeEnv(i)">×</button>
          </div>
        </div>

        <div class="cf-row2">
          <div class="cf-field" style="flex:2">
            <label>{{ t('commandOptional') }}</label>
            <input v-model="form.command" class="cf-input" placeholder="npm start" />
          </div>
          <div class="cf-field" style="flex:1">
            <label>{{ t('workdirOptional') }}</label>
            <input v-model="form.workdir" class="cf-input" placeholder="/app" />
          </div>
        </div>

        <div class="cf-row2">
          <div class="cf-field" style="flex:1">
            <label>{{ t('restartPolicyLabel') }}</label>
            <select v-model="form.restart_policy" class="cf-input">
              <option value="">{{ t('restartNo') }}</option>
              <option value="on-failure">{{ t('restartOnFailure') }}</option>
              <option value="always">{{ t('restartAlways') }}</option>
              <option value="unless-stopped">{{ t('restartUnlessStopped') }}</option>
            </select>
          </div>
          <div class="cf-field" style="flex:1">
            <label>{{ t('networkOptional') }}</label>
            <input v-model="form.network" class="cf-input" placeholder="bridge" :disabled="linkDb" />
          </div>
        </div>

        <div v-if="mode === 'create'" class="cf-section cf-linkdb">
          <label class="cf-checkbox-row">
            <input type="checkbox" v-model="linkDb" />
            <span>{{ t('linkDatabase') }}</span>
          </label>
          <div v-if="linkDb" class="cf-linkdb-body">
            <div class="cf-hint">{{ t('linkDatabaseHint') }}</div>
            <div class="cf-row2">
              <div class="cf-field" style="flex:1">
                <label>{{ t('dbEngineLabel') }}</label>
                <select v-model="linkDbEngine" class="cf-input" @change="onLinkDbEngineChange">
                  <option value="mariadb">MariaDB</option>
                  <option value="postgres">PostgreSQL</option>
                </select>
              </div>
              <div class="cf-field" style="flex:1">
                <label>{{ t('hostPort') }}</label>
                <input v-model="linkDbPort" class="cf-input" />
              </div>
            </div>
            <div class="cf-row2">
              <div class="cf-field" style="flex:1">
                <label>{{ t('fieldDatabase') }}</label>
                <input v-model="linkDbName" class="cf-input" />
              </div>
              <div class="cf-field" style="flex:1">
                <label>{{ t('password') }}</label>
                <input v-model="linkDbPassword" class="cf-input" type="text" />
              </div>
            </div>
          </div>
        </div>

        <div v-if="mode === 'create'" class="cf-section">
          <button v-if="!showSaveTemplate" class="cf-add-row-btn" @click="showSaveTemplate = true">+ {{ t('saveAsTemplate') }}</button>
          <div v-else class="cf-row">
            <input v-model="newTemplateName" class="cf-input" :placeholder="t('templateNamePlaceholder')" />
            <button class="cf-template-btn" @click="saveAsTemplate">{{ t('save') }}</button>
            <button class="cf-row-del" @click="showSaveTemplate = false">×</button>
          </div>
        </div>

        <div v-if="saveError" class="cf-inline-error">{{ saveError }}</div>
      </div>

      <div class="cf-foot">
        <button class="cf-cancel-btn" @click="emit('close')">{{ t('cancel') }}</button>
        <button class="cf-save-btn" @click="submit" :disabled="saving || !form.name.trim() || !form.image.trim()">
          <span v-if="saving" class="cf-spin" />
          <span v-else>{{ mode === 'edit' ? t('save') : t('create') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cf-overlay {
  position: fixed; inset: 0; z-index: 99999; background: rgba(0,0,0,0.6);
  display: flex; align-items: center; justify-content: center; backdrop-filter: blur(3px);
}
.cf-modal {
  background: var(--bg-panel); border: 1px solid var(--border); border-radius: 12px;
  width: 460px; max-height: 86vh; display: flex; flex-direction: column;
  box-shadow: 0 20px 60px rgba(0,0,0,0.6); font-family: var(--font-ui);
}
.cf-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 16px; border-bottom: 1px solid var(--border);
  font-size: 13.5px; font-weight: 700; color: var(--fg-bright); flex-shrink: 0;
}
.cf-close { background: none; border: none; color: var(--fg-muted); font-size: 18px; cursor: pointer; line-height: 1; }
.cf-close:hover { color: var(--fg); }

.cf-body { flex: 1; overflow-y: auto; padding: 14px 16px; display: flex; flex-direction: column; gap: 12px; }
.cf-field { display: flex; flex-direction: column; gap: 4px; }
.cf-field label { font-size: 10.5px; color: var(--fg-muted); font-weight: 600; }
.cf-input {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 12px; padding: 6px 8px; outline: none; font-family: var(--font-ui);
  box-sizing: border-box; width: 100%;
}
.cf-input:focus { border-color: var(--accent); }
.cf-input:disabled { opacity: 0.6; }
.cf-input-sm { width: 70px; flex: 1; }
.cf-input-xs { width: 62px; flex: none; }
.cf-row2 { display: flex; gap: 10px; }

.cf-image-row { display: flex; gap: 6px; }
.cf-image-row .cf-input { flex: 1; }
.cf-pull-btn {
  flex-shrink: 0; background: var(--bg-hover); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 11px; font-weight: 600; padding: 0 12px; cursor: pointer;
  display: flex; align-items: center; justify-content: center; min-width: 70px;
}
.cf-pull-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
.cf-pull-btn:disabled { opacity: 0.5; cursor: default; }

.cf-section { display: flex; flex-direction: column; gap: 6px; }
.cf-section-head { display: flex; align-items: center; justify-content: space-between; }
.cf-section-head > span { font-size: 10.5px; color: var(--fg-muted); font-weight: 600; }
.cf-add-row-btn {
  background: none; border: none; color: var(--accent); font-size: 10.5px;
  cursor: pointer; padding: 2px 4px;
}
.cf-add-row-btn:hover { text-decoration: underline; }

.cf-template-row { display: flex; gap: 6px; flex-wrap: wrap; }
.cf-template-btn {
  background: var(--bg-mid); border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg); font-size: 11.5px; font-weight: 600; padding: 6px 12px; cursor: pointer;
}
.cf-template-btn:hover { border-color: var(--accent); color: var(--accent); }

.cf-custom-template { display: flex; align-items: stretch; }
.cf-custom-template .cf-template-btn { border-radius: 6px 0 0 6px; }
.cf-custom-template-del {
  background: var(--bg-mid); border: 1px solid var(--border); border-left: none;
  border-radius: 0 6px 6px 0; color: var(--fg-muted); font-size: 13px; cursor: pointer; padding: 0 8px;
}
.cf-custom-template-del:hover { color: #f85149; }

.cf-hint { font-size: 10px; color: var(--fg-muted); line-height: 1.4; margin-top: -2px; }

.cf-linkdb { border: 1px solid var(--border); border-radius: 8px; padding: 10px; gap: 8px; }
.cf-checkbox-row { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--fg); cursor: pointer; }
.cf-linkdb-body { display: flex; flex-direction: column; gap: 8px; margin-top: 8px; }

.cf-row { display: flex; align-items: center; gap: 6px; }
.cf-row .cf-input { flex: 1; min-width: 0; }
.cf-sep { color: var(--fg-muted); font-size: 12px; flex-shrink: 0; }
.cf-row-del {
  width: 20px; height: 20px; flex-shrink: 0; background: none; border: none;
  color: var(--fg-muted); font-size: 14px; cursor: pointer; border-radius: 4px;
  display: flex; align-items: center; justify-content: center;
}
.cf-row-del:hover { color: #f85149; background: var(--bg-hover); }

.cf-inline-error {
  font-size: 11px; color: #f85149; padding: 6px 8px;
  background: rgba(248,81,73,0.1); border-radius: 5px;
}

.cf-foot { display: flex; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); flex-shrink: 0; }
.cf-cancel-btn {
  flex: 1; background: none; border: 1px solid var(--border); border-radius: 6px;
  color: var(--fg-muted); font-size: 12px; padding: 7px; cursor: pointer;
}
.cf-cancel-btn:hover { border-color: var(--fg-muted); color: var(--fg); }
.cf-save-btn {
  flex: 2; background: var(--accent); border: none; border-radius: 6px;
  color: var(--accent-fg); font-size: 12px; font-weight: 600; padding: 7px; cursor: pointer;
  display: flex; align-items: center; justify-content: center;
}
.cf-save-btn:hover:not(:disabled) { opacity: 0.85; }
.cf-save-btn:disabled { opacity: 0.5; cursor: default; }
.cf-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.4);
  border-top-color: var(--accent-fg); border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
