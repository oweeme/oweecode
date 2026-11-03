# Changelog

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
