<div align="center">

<img src="oweemedev/src-tauri/icons/128x128.png" width="96" height="96" style="border-radius: 20px;" />

# OweemeIDE

**El IDE moderno para el desarrollador que quiere todo en un solo lugar**

*Ligero · Rápido · Construido con Rust*

[![Versión](https://img.shields.io/github/v/release/oweeme/oweeide?label=versión&color=2e9e87)](https://github.com/oweeme/oweeide/releases/latest)
[![Plataforma](https://img.shields.io/badge/plataforma-Linux%20%7C%20Windows%20%7C%20macOS-blue)](https://github.com/oweeme/oweeide/releases/latest)
[![Licencia](https://img.shields.io/badge/licencia-MIT-green)](LICENSE)

[⬇️ Descargar](#instalación) · [✨ Características](#características) · [📖 Manual de uso](#manual-de-uso) · [🌐 oweeme.com](https://oweeme.com)

</div>

---

## ¿Qué es OweemeIDE?

OweemeIDE es un entorno de desarrollo integrado (IDE) de escritorio creado por **Héctor Martínez** ([oweeme.com](https://oweeme.com)), pensado para las necesidades reales del desarrollador moderno. Construido con **Tauri 2**, **Vue 3** y **Rust**, combina la velocidad nativa de las aplicaciones de escritorio con la flexibilidad del ecosistema web.

A diferencia de editores pesados que consumen gigabytes de RAM, OweemeIDE es **ligero, rápido y tiene todo lo que necesitas** sin plugins adicionales: editor, terminal, base de datos, FTP, cliente API y asistente de IA — todo integrado.

---

## Características

### ✏️ Editor de código avanzado
- Resaltado de sintaxis para **JS, TS, Vue, PHP, Go, Python, Rust, HTML, CSS, SQL, C++** y más
- Autocompletado inteligente
- Búsqueda y reemplazo en el archivo (`Ctrl+F`)
- Búsqueda en todo el proyecto (`Ctrl+Shift+F`)
- Múltiples tabs abiertos simultáneamente
- Navegación `Ctrl+Tab` entre archivos abiertos
- Zoom de fuente (`Ctrl+` / `Ctrl-`)
- Word wrap togglable

### 🗂️ Explorador de archivos
- Árbol de archivos con expansión/colapso
- Crear, renombrar, eliminar archivos y carpetas
- Drag & drop para mover archivos
- Mostrar/ocultar archivos ocultos (`.env`, `.htaccess`…)
- **Navegación por teclado**: `↑↓` mover · `Enter` abrir · `→←` expandir/colapsar · `F2` renombrar
- Reveal automático del archivo activo en el árbol

### 💾 Base de datos integrada
Compatible con **MySQL, MariaDB, PostgreSQL y SQLite** sin instalar nada extra.
- Explorador visual de tablas y conexiones
- Vista de datos con paginación
- **Edición inline** de filas (doble clic para editar)
- **Insertar** nuevas filas con formulario visual
- **Eliminar** filas con confirmación
- **Vista de estructura** de la tabla (columnas, tipos, índices, claves)
- **Editar estructura**: añadir, modificar y eliminar columnas (ALTER TABLE)
- **Crear tablas** desde el IDE con constructor visual
- Editor SQL con ejecución de consultas personalizadas
- Soporte completo para tipos: `FLOAT`, `TEXT`, `DATE`, `DECIMAL`, `JSON`, `BLOB` y más

### 🖥️ Terminal integrada
- Terminal real con **PTY** (no simulada)
- Múltiples sesiones de terminal simultáneas
- Aliases útiles: `ll`, `la`, `lt`, `grep` con colores
- Comando `e <archivo>` para abrir archivos directamente en el editor desde la terminal
- Soporte completo de colores ANSI

### 🐳 Gestión de contenedores
Compatible con **Podman** (preferido) y **Docker**, sin salir del IDE.
- Crear, iniciar, detener, reiniciar y eliminar contenedores desde un formulario visual
- Plantillas por lenguaje (Node, PHP, Python, Java, C#, Rust, Go) que montan el proyecto abierto automáticamente
- Stack **Nginx + PHP-FPM** con un clic, generando la configuración de Nginx dentro del proyecto
- Vincular una base de datos (MariaDB/PostgreSQL) a un contenedor de app en la misma red
- Ver logs en vivo, editar la configuración de un contenedor existente
- **Exportar** un contenedor a `.tar` (commit + save) para llevarlo a otra máquina
- Filtro "solo este proyecto" para encontrar rápido los contenedores que le corresponden a la carpeta abierta

### 🤖 Asistente de IA
Conecta con los mejores modelos de IA directamente desde el IDE:
- **Claude** (Anthropic) — Sonnet, Opus, Haiku
- **DeepSeek** — DeepSeek Coder, DeepSeek Chat
- **Gemini** (Google) — 2.0 Flash, 1.5 Pro
- **OpenAI** — GPT-4o, GPT-4 Turbo
- **Groq** — inferencia ultra rápida, capa gratis generosa
- **OpenRouter** — catálogo de modelos gratis consultado en vivo
- **Ollama** — modelos locales sin internet, y modelos cloud de Ollama (login opcional)
- Contexto automático del archivo/selección activa en cada consulta
- Memoria de proyecto compartida (`AGENTS.md`) — el resumen de cada conversación queda en el repo, así lo puede leer cualquier otra herramienta de IA (Claude Code, Aider, etc.)

### ⬡ Agente de IA local (Ollama)
Un agente con acceso real al proyecto, corriendo 100% en tu máquina — pensado para no depender de ninguna API paga ni mandar tu código a ningún servidor externo.
- Pestaña propia en el área de trabajo (menú de IA ⚡ → "Agente (Ollama)")
- Lee, busca y escribe archivos del proyecto; usa git (status, diff, stage, commit); ejecuta comandos de shell puntuales
- Crea y gestiona contenedores Podman/Docker para el proyecto (ver sección de Contenedores) — etiquetados automáticamente para poder filtrarlos después
- **Toda acción que modifique algo pide tu aprobación explícita antes de ejecutarse** — nunca escribe, borra ni corre nada sin confirmación
- Confinado a la carpeta del proyecto abierto — no puede tocar archivos fuera de ahí
- Instalación guiada de Ollama y descarga de modelos con progreso en vivo, incluidos los recomendados (`qwen2.5-coder` 3B y 7B) y cualquier otro modelo que tu hardware soporte — accesible desde la pestaña del Agente o desde **Preferencias**
- Soporte para modelos cloud de Ollama (`ollama signin`) si preferís no bajar nada a disco

### ◆ Claude Code CLI (workspace)
- Abre **Claude Code CLI** como tab en el área de trabajo (`Ctrl+Shift+C`)
- Igual que la extensión de VS Code pero en OweemeIDE
- Botón "Enviar archivo" — pasa el contexto del archivo activo al CLI
- Usa tu suscripción Claude Pro sin API key adicional

### ◈ Gemini CLI (workspace)
- Abre **Gemini CLI** como tab en el área de trabajo (`Ctrl+Shift+G`)
- Autenticado con tu cuenta de Google
- Misma experiencia que la terminal pero embebida en el IDE

### ✦ Aider (workspace)
- CLI de código abierto para pares de programación asistida por IA, embebido como tab
- **Selector de proveedor y modelo propio**, independiente del panel de Chat — cada uno con su clave guardada por separado, así podés tener Chat y Aider usando modelos distintos a la vez
- Funciona con cualquiera de los proveedores soportados, incluido Ollama en local

### GitHub Copilot CLI (workspace)
- Abre **Copilot CLI** como tab en el área de trabajo
- Requiere una cuenta de GitHub con Copilot (hay plan gratuito limitado)

### 📡 Cliente FTP / SFTP
- Conexiones FTP y SFTP guardadas
- Explorador de archivos remotos
- Subir, descargar, crear y eliminar archivos/carpetas remotas
- Edición directa de archivos remotos en el editor

### ⚡ Cliente API REST
- Constructor de peticiones HTTP (GET, POST, PUT, DELETE, PATCH)
- Headers, params, body (JSON, form-data, raw)
- Colecciones de requests guardadas
- Visualizador de respuesta con formato JSON

---

## Instalación

### ⬇️ Descargar el instalador

Ve a la página de [Releases](https://github.com/oweeme/oweeide/releases/latest) y descarga el instalador para tu sistema:

| Sistema | Archivo |
|---------|---------|
| 🐧 Linux (Debian/Ubuntu/Neon/Mint) | `OweemeIDE_x.x.x_amd64.deb` |
| 🐧 Linux (cualquier distro) | `OweemeIDE_x.x.x_amd64.AppImage` |
| 🪟 Windows | `OweemeIDE_x.x.x_x64-setup.exe` |
| 🍎 macOS | `OweemeIDE_x.x.x_x64.dmg` |

### 🐧 Linux — instalación con .deb (recomendado)

```bash
# Instalar
sudo dpkg -i OweemeIDE_*.deb

# Si falta alguna dependencia
sudo apt-get install -f
```

Después aparece en tu menú de aplicaciones como **OweemeIDE**.

### 🐧 Linux — AppImage (sin instalación)

```bash
chmod +x OweemeIDE_*.AppImage
./OweemeIDE_*.AppImage
```

### 🪟 Windows

1. Descarga el archivo `.exe`
2. Ejecuta como administrador
3. Sigue el instalador
4. Si Windows Defender muestra una advertencia: clic en **"Más información"** → **"Ejecutar de todas formas"**

### 🍎 macOS

1. Descarga el archivo `.dmg`
2. Abre el `.dmg` y arrastra **OweemeIDE** a la carpeta **Aplicaciones**
3. Si aparece advertencia de seguridad: **Preferencias del Sistema** → **Seguridad** → **"Abrir de todas formas"**

---

## Manual de uso

### Abrir un proyecto

1. Clic en el ícono de **Explorador** en la barra lateral izquierda (o `Ctrl+B`)
2. Clic en el ícono de carpeta junto al nombre del proyecto
3. Selecciona la carpeta raíz de tu proyecto
4. El árbol de archivos se cargará automáticamente

### Editar archivos

- **Abrir**: clic en cualquier archivo en el explorador, o `Enter` con el teclado
- **Guardar**: `Ctrl+S`
- **Cerrar tab**: clic en `×` del tab, o `Ctrl+W`
- **Buscar en archivo**: `Ctrl+F`
- **Buscar en proyecto**: `Ctrl+Shift+F`
- **Navegar entre tabs**: `Ctrl+Tab` (siguiente) / `Ctrl+Shift+Tab` (anterior)

### Conectar a una base de datos

1. Clic en el ícono de **Base de datos** en la barra lateral
2. Clic en **"+ Nueva conexión"**
3. Selecciona el driver: MySQL / MariaDB / PostgreSQL / SQLite
4. Ingresa host, puerto, usuario, contraseña y base de datos
5. Clic en **Conectar**
6. Las tablas aparecerán en el árbol — clic para abrirlas como tabs

### Usar el Asistente de IA

1. Clic en el ícono de **IA** en la barra lateral (o `Ctrl+Shift+A`)
2. Selecciona el proveedor (Claude, DeepSeek, Gemini, OpenAI u Ollama)
3. Ingresa tu API key (solo la primera vez)
4. Escribe tu consulta y presiona `Enter`

**Tip:** Selecciona código en el editor antes de preguntar — el AI recibe el contexto automáticamente.

### Usar el Agente de IA local (Ollama)

1. Clic en el menú de IA (⚡) en la barra lateral → **"Agente (Ollama)"**
2. Si es la primera vez, clic en el botón ⚙ del header (o en **Preferencias**) para instalar Ollama y bajar los modelos recomendados
3. Elegí el modelo en el selector del header de la pestaña
4. Escribí lo que necesitás — leer/escribir archivos, correr un comando, crear un contenedor, etc.
5. Cuando el agente proponga una acción que modifica algo, vas a ver una tarjeta de confirmación — **Aprobar** o **Rechazar** antes de que se ejecute

> Para tareas de varios pasos encadenados (por ejemplo, un contenedor de app vinculado a uno de base de datos), conviene pedirlas de a un paso por mensaje — los modelos locales chicos son más confiables así.

### Usar Claude Code CLI

1. Clic en el botón **◆** (naranja) en la barra lateral, o `Ctrl+Shift+C`
2. Se abre como tab en el área de trabajo
3. Interactúa con Claude Code directamente
4. Clic en **"Enviar archivo"** para pasar el archivo activo como contexto

> Requiere `claude` instalado: `npm install -g @anthropic-ai/claude-code`

### Usar Gemini CLI

1. Clic en el botón **◈** (azul) en la barra lateral, o `Ctrl+Shift+G`
2. Se abre como tab en el área de trabajo

> Requiere `gemini` instalado: `npm install -g @google/gemini-cli`

### Conectar por FTP / SFTP

1. Clic en el ícono de **FTP** en la barra lateral
2. Clic en **"+ Nueva conexión"**
3. Ingresa host, puerto, usuario y contraseña
4. Selecciona protocolo FTP o SFTP
5. Clic en **Conectar** — se abre el explorador remoto como tab

### Hacer peticiones API

1. Clic en el ícono de **API** (rayo) en la barra lateral
2. Clic en **"+ Nueva petición"**
3. Selecciona método (GET, POST, PUT…) e ingresa la URL
4. Agrega headers, params o body según necesites
5. Clic en **Enviar** — la respuesta aparece en el panel inferior

---

## Atajos de teclado

| Atajo | Acción |
|-------|--------|
| `Ctrl+B` | Mostrar/ocultar explorador |
| `Ctrl+S` | Guardar archivo |
| `Ctrl+W` | Cerrar tab activo |
| `Ctrl+Tab` | Siguiente tab |
| `Ctrl+Shift+Tab` | Tab anterior |
| `Ctrl+F` | Buscar en archivo |
| `Ctrl+Shift+F` | Buscar en proyecto |
| `Ctrl+\`` | Abrir/cerrar terminal |
| `Ctrl+Shift+A` | Abrir AI Assistant |
| `Ctrl+Shift+C` | Abrir Claude Code CLI |
| `Ctrl+Shift+G` | Abrir Gemini CLI |
| `Ctrl+=` | Aumentar zoom |
| `Ctrl+-` | Reducir zoom |
| `Ctrl+0` | Resetear zoom |
| `Alt+Z` | Toggle word wrap |
| `F2` | Renombrar archivo (en explorador) |
| `Escape` | Cerrar menús / paneles |

---

## Tecnología

OweemeIDE está construido con tecnología moderna y de alto rendimiento:

| Capa | Tecnología |
|------|-----------|
| Backend / Sistema | **Rust** + Tauri 2 |
| Frontend / UI | **Vue 3** + TypeScript |
| Editor de código | **CodeMirror 6** |
| Terminal | **xterm.js** + PTY nativo |
| Base de datos | **sqlx** (MySQL/PostgreSQL/SQLite) |
| Empaquetado | Tauri Bundler (.deb, .exe, .dmg) |

---

## Soporte y contacto

¿Tienes dudas, sugerencias o encontraste un bug?

- 📧 **Email**: [hector@oweeme.com](mailto:hector@oweeme.com)
- 🌐 **Web**: [oweeme.com](https://oweeme.com)
- 🐛 **Issues**: [github.com/oweeme/oweeide/issues](https://github.com/oweeme/oweeide/issues)

---

<div align="center">

Desarrollado con ❤️ por **Héctor Martínez** — [oweeme.com](https://oweeme.com)

</div>
