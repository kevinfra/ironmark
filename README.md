# ironmark

Visualizador de Markdown liviano, rápido, seguro y de **solo lectura** (sin editor), escrito en Rust, con soporte completo de **Mermaid 100% offline** para **Linux, macOS y Windows**.

## Características

- **Solo lectura, cero editor**: abre archivos `.md` al instante sin bloating ni Electron (usa el WebView nativo del sistema vía `tao` + `wry`).
- **Renderizado seguro (defensa en profundidad)**:
  - Parseo de CommonMark + GFM (tablas, task lists, notas al pie, tachado, slugs de encabezados) con `pulldown-cmark`.
  - Sanitización HTML estricta en Rust con `ammonia` (elimina `<script>`, `<iframe>`, `<svg>`, `<style>`, `<form>`, atributos `on*`, URLs `javascript:` / `vbscript:` / `file:` / `data:`).
  - Protocolo personalizado `ironmark://` (sin abrir puertos locales) con cabecera HTTP `Content-Security-Policy` estricta (`script-src 'self'`, `object-src 'none'`, `frame-src 'none'`).
  - Imágenes locales relativas servidas desde `/local/*` con verificación de ruta canónica dentro del directorio del documento (sin *path traversal*) y `Content-Security-Policy: sandbox`.
  - Navegación externa bloqueada dentro del WebView; enlaces `http(s)://` / `mailto:` se abren en el navegador del sistema y enlaces relativos a otros `.md` se abren directamente en `ironmark`.
- **Mermaid 100% offline**: `mermaid.min.js` embebido en el binario (comprimido con gzip, sin CDN) ejecutándose con `securityLevel: 'strict'`. Haz clic en cualquier diagrama para expandirlo y hacer zoom.
- **Privacidad offline por defecto (imágenes remotas opcionales)**: las imágenes `http(s)://` se bloquean por defecto tanto en la sanitización como en la CSP. Se pueden habilitar opcionalmente con `--allow-remote` o desde el botón **Offline** en la barra superior.
- **Recarga automática en disco**: observa el archivo abierto (incluyendo guardados atómicos de Vim, VS Code, Helix, etc.) y actualiza el DOM sin parpadeos manteniendo la posición de scroll.
- **Arrastrar y soltar + Diálogo nativo + CLI**: abre archivos desde terminal, arrastrándolos a la ventana o con `Cmd/Ctrl+O`.
- **Modo claro / oscuro**: sigue el tema del sistema automáticamente con selector manual (`Auto` / `Light` / `Dark`) que también re-renderiza los diagramas Mermaid.

## Uso

```bash
# Abrir un archivo Markdown
ironmark README.md

# Abrir permitiendo imágenes remotas http(s)
ironmark --allow-remote README.md

# Abrir ventana vacía (para arrastrar o elegir archivo)
ironmark
```

### Atajos de teclado

| Atajo | Acción |
| :--- | :--- |
| `Cmd/Ctrl + O` | Abrir archivo `.md` |
| `Cmd/Ctrl + F` | Buscar en el documento |
| `Cmd/Ctrl + R` | Recargar manualmente |
| `Cmd/Ctrl + +` / `-` / `0` | Zoom in / out / 100% |
| `Cmd/Ctrl + Q` | Salir |
| `Esc` | Cerrar búsqueda o modal de diagrama |

## Compilación

### Requisitos del sistema

- **macOS**: Xcode Command Line Tools (`WKWebView` incluido en el SO).
- **Windows**: MSVC toolchain + WebView2 Runtime (incluido en Windows 10/11).
- **Linux**: GTK 3 y WebKitGTK:
  - Debian / Ubuntu:
    ```bash
    sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev
    ```
  - Fedora:
    ```bash
    sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel
    ```
  - Arch Linux:
    ```bash
    sudo pacman -S gtk3 webkit2gtk-4.1
    ```

### Build optimizado (release)

```bash
cargo build --release
```

El binario compilado queda en `target/release/ironmark` (`target\release\ironmark.exe` en Windows).

### Probar con el archivo de ejemplo

```bash
cargo run -- examples/demo.md
```

## Contribuciones

Las contribuciones son bienvenidas. Consultá [CONTRIBUTING.md](CONTRIBUTING.md) para ver cómo compilar, correr los tests y enviar pull requests.

## Licencia y terceros

`ironmark` se distribuye bajo la licencia [MIT](LICENSE).

Los avisos de licencias de terceros (incluido el bundle offline de Mermaid con versión y hash SHA-256) están documentados en [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
