// Some labels come straight from the Rust side (dev-server suggestions, mobile/Qt commands, project doctor) written in
// Spanish. Spanish users see them as they are; every other language gets the English text from this table.
// A text that is not in the table is shown unchanged, so a new Rust label never disappears.

import { RUST_TR } from './rustText.langs'

const EXACT: Record<string, string> = {
  // commands
  'Abrir en Android Studio': 'Open in Android Studio', 'Abrir en el editor de Godot': 'Open in the Godot editor', 'Abrir Glade / Cambalache': 'Open Glade / Cambalache',
  'Abrir Qt Designer': 'Open Qt Designer', 'Agregar plataforma Android': 'Add Android platform', 'Analizar código': 'Analyze code', 'Compilar': 'Build',
  'Compilar AAB (Play Store)': 'Build AAB (Play Store)', 'Compilar APK': 'Build APK', 'Compilar APK/AAB (Quasar)': 'Build APK/AAB (Quasar)', 'Compilar debug': 'Build debug',
  'Compilar la web': 'Build the web app', 'Compilar recursos (.qrc)': 'Compile resources (.qrc)', 'Compilar release (AAB)': 'Build release (AAB)',
  'Configurar (Debug + compile_commands para clangd)': 'Configure (Debug + compile_commands for clangd)', 'Configurar (meson)': 'Configure (meson)',
  'Configurar para Android (Qt for Android + NDK)': 'Configure for Android (Qt for Android + NDK)', 'Convertir .ui a Python': 'Convert .ui to Python', 'Dependencias': 'Dependencies',
  'Desarrollo en Android': 'Android development', 'Desarrollo en Android (Quasar)': 'Android development (Quasar)', 'Diagnóstico': 'Diagnostics', 'Diagnóstico de Capacitor': 'Capacitor diagnostics',
  'Dispositivos': 'Devices', 'Ejecutar': 'Run', 'Ejecutar el juego': 'Run the game', 'Ejecutar en Android': 'Run on Android', 'Ejecutar en dispositivo/emulador': 'Run on device/emulator',
  'Ejecutar (gtk4-rs)': 'Run (gtk4-rs)', 'Ejecutar (PyGObject)': 'Run (PyGObject)', 'Empaquetar en Windows (Qt Deploy)': 'Package on Windows (Qt Deploy)', 'Empaquetar la app': 'Package the app',
  'Exportar (necesita presets en Godot)': 'Export (needs presets in Godot)', 'Instalar en dispositivo': 'Install on device', 'Instalar GTK4 para Python (Ubuntu)': 'Install GTK4 for Python (Ubuntu)',
  'Instalar PySide6': 'Install PySide6', 'Limpiar': 'Clean', 'Limpiar carpeta build': 'Clean the build folder', 'Logs (errores)': 'Logs (errors)', 'Probar un .ui con el visor': 'Try a .ui with the viewer',
  'Revisar scripts (sin ventana)': 'Check scripts (headless)', 'Sincronizar con Android': 'Sync with Android', 'Tests unitarios': 'Unit tests', 'Versión de Godot': 'Godot version',
  // project kinds
  'Tauri móvil': 'Tauri mobile', 'Android nativo (Kotlin/Java)': 'Native Android (Kotlin/Java)', 'Qt / QML (escritorio)': 'Qt / QML (desktop)', 'Python + Qt (PySide/PyQt)': 'Python + Qt (PySide/PyQt)',
  // doctor
  'Proyecto': 'Project', 'Sin tipo de proyecto detectado': 'No project type detected', 'OweeCode reconoce Capacitor, Android, Flutter, Qt, GTK, PySide, Godot y React Native': 'OweeCode recognizes Capacitor, Android, Flutter, Qt, GTK, PySide, Godot and React Native',
  'GTK 4 para Python (PyGObject)': 'GTK 4 for Python (PyGObject)', 'pycairo (dibujar en GTK)': 'pycairo (drawing in GTK)', 'Compilador C++ (g++)': 'C++ compiler (g++)', 'clangd (autocompletado C++)': 'clangd (C++ completion)',
  'qmlls (autocompletado QML)': 'qmlls (QML completion)', 'Viene con Node.js': 'Comes with Node.js', 'Viene con Flutter': 'Comes with Flutter', 'El proyecto trae ./gradlew; no hace falta instalarlo': 'The project ships ./gradlew; no need to install it',
  'Viene con Qt 6.3+ (~/Qt/<versión>/<kit>/bin)': 'Comes with Qt 6.3+ (~/Qt/<version>/<kit>/bin)', 'Instalá Android Studio (descarga el SDK) y definí ANDROID_HOME=~/Android/Sdk': 'Install Android Studio (it downloads the SDK) and set ANDROID_HOME=~/Android/Sdk',
  'Instalá Qt con el Qt Online Installer (https://www.qt.io/download); OweeCode lo busca en ~/Qt': 'Install Qt with the Qt Online Installer (https://www.qt.io/download); OweeCode looks for it in ~/Qt',
  'SDK de Flutter o Dart (incluye el servidor)': 'Flutter or Dart SDK (includes the server)', 'viene con Qt (~/Qt/<versión>/<kit>/bin)': 'comes with Qt (~/Qt/<version>/<kit>/bin)',
  // dev servers
  'Docker Compose: levantar servicios': 'Docker Compose: start services', 'PHP: servidor integrado': 'PHP: built-in server', 'Servidor incluido en OweeCode': 'Server built into OweeCode',
  'Servidor incluido en OweeCode, sin instalar nada': 'Server built into OweeCode, nothing to install', 'Servidor integrado de PHP': 'PHP built-in server', 'Sirve el proyecto con el servidor de PHP': 'Serves the project with the PHP server',
  'Ajustá main:app si tu archivo se llama distinto': 'Change main:app if your file has another name', 'Puerto no detectado': 'Port not detected', 'qmake + compilar': 'qmake + build',
}

// patterns with a variable part: English template + the value; the template is what the other languages translate
const PATTERNS: [RegExp, string][] = [
  [/^Servir (.+) \(HTML estático\)$/, 'Serve {x} (static HTML)'],
  [/^PHP: servir (.+)$/, 'PHP: serve {x}'],
  [/^Puertos publicados: (.+)$/, 'Published ports: {x}'],
]

/** Spanish stays as it is; English comes from the tables above; the other languages translate the English text */
export function rustText(text: string, locale: string): string {
  if (locale === 'es' || !text) return text
  const table = RUST_TR[locale]
  const tr = (en: string) => (table && table[en]) || en
  const hit = EXACT[text]
  if (hit) return tr(hit)
  for (const [re, tpl] of PATTERNS) {
    const m = text.match(re)
    if (!m) continue
    const x = m[1] === 'la carpeta del proyecto' ? tr('the project folder') : m[1]
    return tr(tpl).split('{x}').join(x)
  }
  return text
}
