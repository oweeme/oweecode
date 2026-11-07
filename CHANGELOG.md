# Changelog

## Sin publicar (rama `oweeme-chat`)

### Novedades
- **Chat de equipo de Oweeme dentro del editor:** nuevo panel **Oweeme** en la barra lateral. Te conectas con un token personal `owc_` (se crea en oweeme.com → Chat de equipo → Tokens) que se guarda en el llavero del sistema y nunca pasa por la interfaz web; las peticiones salen desde Rust.
- La sala cuyo repositorio coincide con el `remote` de Git del proyecto abierto se abre sola (SSH, HTTPS y `.git` se reconocen como el mismo repositorio).
- **Adjuntar selección:** envía el código seleccionado con su archivo y rama como contexto.
- Crear salas desde el editor (con el repositorio del proyecto ya rellenado) y elegir servidor: oweeme.com, `oweeme_pod` local u otro.
- **Compartir en el chat:** código seleccionado, el archivo activo completo, tu flujo de la pestaña Flujos y tu vista previa del navegador (con la dirección de tu red local para que el equipo la abra). Lo que comparten los demás se abre con un clic: abrir el archivo en tu proyecto, importar el flujo o abrir la vista previa.
- **Pestaña Tareas:** el tablero del proyecto vinculado a la sala. Abrir una tarea, comentarla y moverla de columna; el cliente ve el cambio en su propuesta de Oweeme.
- **Tareas y Git:** la rama `bella-7-paypal` muestra «Trabajando en BELLA-7»; tras un commit que menciona la tarea, OweeCode ofrece comentarlo en ella y pasarla a Revisión.

## 1.5.0

### Novedades
- **Navegador integrado con proyectos:** detecta los servidores que corren en cualquier puerto (80, 3000, 8000, 9000…), sugiere cómo arrancar los de tu proyecto, sirve carpetas HTML estáticas, vista dividida de dos puertos, recarga automática al guardar (y cambio de CSS en vivo sin recargar), y favoritos por proyecto.
- **FTP / SFTP / FTPS como FileZilla:** navegación por teclado, cola de transferencias con pausa y reintento, confirmación antes de sobrescribir, **sincronización de carpetas** (comparar, revisar y aplicar), y un botón de desconectar que cierra la pestaña.
- **Seguridad:** verificación de claves de host SFTP, FTPS con certificados fijados, contraseñas en el llavero del sistema y política de seguridad de contenido (CSP).
- **Flujos de negocio:** diseña procesos conectando bloques, ejecútalos con scripts en cualquier lenguaje, siembra datos de ejemplo en tu base, genera código base en 10 lenguajes y mapea tu proyecto abriendo cada archivo.
- **Diseño visual:** edita archivos Quasar/Vue/HTML directamente y un diseñador de interfaces nativas para **Qt (C++/QML/PySide), GTK, Godot y juegos**.
- **Móvil y escritorio:** diagnóstico del proyecto para Capacitor, Android, Flutter, Qt, GTK y Godot.
- **Idiomas:** las pantallas nuevas están traducidas a español, inglés, alemán, francés, coreano, chino, japonés y ruso.
- Pantalla de carga con el logo al abrir la aplicación.

### Correcciones
- El explorador vuelve a cargarse al actualizar el proyecto y las Preferencias detectan las herramientas instaladas.
- Subidas FTPS completas (antes se cortaban) y transferencias en modo binario.

## 1.4.2
- Mensaje claro cuando el auto-actualizador no puede actualizar una instalación .deb/.rpm en Linux.
