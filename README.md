[shield_hackathon404.md](https://github.com/user-attachments/files/27093345/shield_hackathon404.md)
# Shield — Entrega Hackathon404

**Equipo:** Blackjack  
**Convocatoria:** Hackathon404 — Seguridad Digital Infantil  
**Repositorio:** [https://github.com/zam-cv/hackathon404](https://github.com/zam-cv/hackathon404)

---

## Nombre del Proyecto y Descripción

**Shield** es una aplicación multiplataforma (móvil y desktop) de navegación segura diseñada para niños y preadolescentes. Funciona como un "sistema operativo simplificado" que expone únicamente un conjunto restringido de apps (Navegador, Facebook simulado, Instagram simulado, Calculadora y Notas), protegiendo al menor de contenido inapropiado mediante filtros activos en tres capas: bloqueo de URLs, censura de texto en tiempo real e inhibición de imágenes.[cite:13] Los tutores pueden monitorear la actividad desde un dashboard de telemetría independiente con KPIs, gráficas y un feed de eventos en vivo.[cite:16]

---

## Problema que Resuelve

Los menores de edad que acceden a internet sin supervisión quedan expuestos a contenido adulto, lenguaje violento, imágenes explícitas y situaciones de grooming. Las soluciones existentes (controles parentales del sistema operativo, extensiones de navegador) son fácilmente eludibles, requieren configuración técnica avanzada por parte del padre/tutor, y no ofrecen visibilidad en tiempo real de lo que el niño está consultando.[cite:13]

Shield resuelve este problema con un enfoque de tres capas:

- **Capa 1 — Bloqueo de URL:** antes de que el WebView cargue cualquier página, el backend en Rust evalúa el dominio contra una lista negra de sitios para adultos; si hay coincidencia, la navegación se cancela y se emite un evento `browser-blocked` que las asociaciones puede ver en un dashboard para generar inteligencia, de manera anonima, solo recopilando datos necesarios para el análisis.
- **Capa 2 — Filtrado de texto en página:** un script JS inyectado en cada WebView intercepta el DOM y reemplaza caracteres en textos que el clasificador de IA marca como inapropiados, haciendo el contenido ilegible sin eliminar la página completa.[cite:13]
- **Capa 3 — Censura de imágenes:** las imágenes marcadas son enviadas al backend Rust, que aplica blur gaussiano real a nivel de bytes antes de devolverlas al WebView; el resultado es una imagen irreversiblemente borrosa.[cite:13]

---

## Tecnologías y Herramientas Utilizadas

### Stack principal

| Capa | Tecnología | Versión | Propósito |
|---|---|---|---|
| App multiplataforma | Tauri | 2.x | Framework desktop/móvil nativo con WebView |
| Frontend UI | React | 19.x | Interfaz del "OS" infantil |
| Lenguaje frontend | TypeScript | ~5.8.3 | Tipado estático del cliente |
| Estilos | Tailwind CSS | 4.x | Diseño visual de la interfaz |
| Iconografía | Lucide React, React Icons | latest | Iconos de apps (Calculadora, Notas, redes) |
| Enrutamiento | React Router DOM | 7.x | Navegación entre apps internas |
| Bundler | Vite | 7.x | Build y dev server del frontend |
| Gestor de paquetes JS | Bun | latest | Instalación y ejecución rápida |
| Backend nativo | Rust (tokio, image, rand) | stable | Blur de imágenes, control del WebView, IPC |
| Clasificador IA | Python + HuggingFace Transformers + PyTorch | 3.11+ | Clasificación NLI zero-shot de texto |
| Modelo IA | MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli | — | Inferencia multilingüe sin fine-tuning |
| Gestión Python | uv | latest | Entorno virtual y dependencias del clasificador |

[cite:5][cite:6][cite:13][cite:15]

### Arquitectura general

```
shield/
├── app/               # Aplicación principal (Tauri + React)
│   ├── src/           # Componentes React (Desktop, Navegador, páginas)
│   └── src-tauri/     # Backend Rust (filtros, WebView nativo)
│       ├── lib.rs     # Comandos IPC: open/navigate/close WebView, filter_texts, filter_image_bytes
│       └── filter.js  # Script JS inyectado en cada WebView (~15KB)
├── classifier/        # Clasificador de IA (Python)
│   ├── main.py        # Pipeline NLI zero-shot con atajos léxicos
│   └── .env.example   # Configuración de categorías, hipótesis y umbrales
└── dashboard/         # Panel de control parental (Tauri + React)
    └── src/           # KPIs, gráficas, tabla raw, feed en vivo
```

[cite:2][cite:12][cite:14]

---

## Instrucciones para Ejecutar el Prototipo

### Prerrequisitos

- [Rust](https://rustup.rs/) (toolchain stable)
- [Node.js](https://nodejs.org/) 20+ y [Bun](https://bun.sh/)
- [Tauri CLI v2](https://tauri.app/start/prerequisites/) (`cargo install tauri-cli`)
- Python 3.11+ con [`uv`](https://github.com/astral-sh/uv) (`pip install uv`)
- Para Android: Android SDK + NDK configurados
- Para iOS: Xcode en macOS

### 1. App principal (Shield)

```bash
git clone https://github.com/zam-cv/hackathon404
cd hackathon404/app

# Instalar dependencias JavaScript
bun install

# Ejecutar en modo desarrollo (desktop)
bun run tauri dev

# Compilar instalador de producción
bun run tauri build
```

Para dispositivos móviles:
```bash
# Android
bun run tauri android dev

# iOS (requiere macOS + Xcode)
bun run tauri ios dev
```

[cite:15]

### 2. Clasificador de IA

```bash
cd hackathon404/classifier

# Copiar y editar configuración
cp .env.example .env
# Editar .env: definir NLI_MODEL, CATEGORY_KEYS, HYPOTHESES_*, LEXICAL_*, THRESHOLDS, TEST_CASES

# Instalar dependencias Python
uv sync

# Ejecutar pruebas del clasificador
uv run python main.py
```

El clasificador usa GPU automáticamente si CUDA está disponible; de lo contrario usa CPU.[cite:5]

### 3. Dashboard parental

```bash
cd hackathon404/dashboard
bun install
bun run tauri dev
```

[cite:14]

---

## Demo del Prototipo



> **Nota:** Para una demostración en vivo, clonar el repositorio y ejecutar `bun run tauri dev` dentro de `app/` siguiendo las instrucciones anteriores.

---

## Documentación Explícita de Herramientas de IA

### Herramienta 1: Modelo NLI Multilingüe (Clasificación de Contenido)

**Herramienta:** `MoritzLaurer/multilingual-MiniLMv2-L6-mnli-xnli` vía HuggingFace `transformers`

**¿Para qué?** Clasificar texto (búsquedas, mensajes, contenido de páginas) como perteneciente a categorías de contenido inapropiado definidas en la configuración (violencia, contenido sexual, grooming, etc.), sin necesidad de un dataset etiquetado específico del dominio.[cite:6]

**¿En qué medida?**

El clasificador opera en tres pasos:[cite:5]

1. **Atajo léxico:** si el texto contiene 2 o más coincidencias contra listas de palabras clave (`frases`, `emojis`, `hashtags`, `regex`) configuradas por categoría, el modelo NLI se omite y se asigna directamente score `0.95` a esa categoría. Esto reduce latencia en los casos más obvios.
2. **Inferencia NLI zero-shot:** si el atajo no se activa, el pipeline evalúa el texto contra todas las hipótesis de todas las categorías con `multi_label=True`. El score final por categoría es el máximo entre sus hipótesis.
3. **Boost léxico parcial:** si hubo exactamente 1 coincidencia léxica, el score se eleva al mínimo `0.70` aunque el modelo haya dado menos.

**Decisión final:** basada en umbrales configurables por categoría:

| Score vs. Umbral | Acción |
|---|---|
| Score < umbral | `PERMITIR` |
| Score ≥ umbral | `AVISAR` (notificación al tutor) |
| Score ≥ umbral + 0.10 | `BLOQUEAR` (contenido censurado) |

[cite:5][cite:6]

---

### Herramienta 2: Filtrado de Imágenes con Blur Gaussiano (Backend IA + Procesamiento)

**Herramienta:** Crate `image` de Rust con `fast_blur` y `resize`, ejecutado vía `tokio::task::spawn_blocking`

**¿Para qué?** Censurar imágenes potencialmente inapropiadas que el clasificador detecta en páginas web abiertas dentro del WebView.[cite:13]

**¿En qué medida?**

El proceso es completamente automatizado:[cite:13]

1. El script `filter.js` inyectado en el WebView intercepta las etiquetas `<img>` de la página
2. Hace `fetch(img.src)` localmente (cache hit del navegador) y envía los bytes brutos al backend Rust vía IPC binario (`invoke("filter_image_bytes", bytes)`)
3. El backend en Rust: redimensiona la imagen a 128×128 px (`FilterType::Triangle`), aplica `fast_blur` con sigma `8.0` (blur separable gaussiano, ~5× más rápido que blur full-2D)
4. Devuelve los bytes JPEG borrosos como `tauri::ipc::Response` (transporte binario, no JSON/base64), evitando overhead de serialización
5. El script JS sustituye el `src` original de la imagen con los bytes recibidos

---

### Herramienta 3: Filtrado de Texto en DOM (Script JS + Comando IPC)

**Herramienta:** Script JavaScript inyectado (`filter.js`) + comando Rust `filter_texts`

**¿Para qué?** Censurar en tiempo real el contenido textual de cualquier página web que el niño visite, sin bloquear la página completa.[cite:13]

**¿En qué medida?**

- El script JS extrae texto de elementos `<p>`, `<h1>`–`<h6>` y otros nodos del DOM
- Envía lotes de textos al backend Rust en una sola llamada IPC (`invoke("filter_texts", { texts })`) para evitar ~100+ invocaciones individuales por página
- El backend en Rust reemplaza aleatoriamente ~30% de los caracteres alfabéticos por guiones (`-`), haciendo el texto ilegible pero visible (el niño percibe que hay contenido pero no puede leerlo)
- La respuesta se aplica de vuelta al DOM en el WebView[cite:13]

---

## Integrantes del Equipo

**Equipo: Blackjack**

| Nombre  | 
|---------|
| Carlos Alberto Zamudio Velázquez | 
| Ivan Alexander Ramos Ramirez |
| Yael Octavio Perez Mendez |
| Sarai Campillo Galicia



---

## Convocatoria

**Hackathon404 — Seguridad Digital Infantil**

El proyecto fue desarrollado íntegramente durante el hackathon, con el primer commit el **24 de abril de 2026** y el último commit el **25 de abril de 2026**.[cite:16]
