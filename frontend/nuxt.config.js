export default {
  // Target: https://go.nuxtjs.dev/config-target
  target: 'server',
  // Disable server side rendering, so that middleware is executed on page refresh and route changes.
  ssr: false,

  // Global page headers: https://go.nuxtjs.dev/config-head
  head: {
    title: 'MAcWorP',
    htmlAttrs: {
      lang: 'en'
    },
    meta: [
      { charset: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { hid: 'description', name: 'description', content: 'Cloud environment for Nextflow' }
    ],
    link: [
      { rel: 'icon', type: 'image/x-icon', href: '/favicon.ico' }
    ]
  },

  // Global CSS: https://go.nuxtjs.dev/config-css
  css: [
    "~/assets/sass/application.sass"
  ],

  // Plugins to run before rendering page: https://go.nuxtjs.dev/config-plugins
  plugins: [
    "~/plugins/ace_editor.js",
    "~/plugins/event_bus.js",
    "~/plugins/api_error_handling.js",
    "~/plugins/bootstrap_modal_control.client.js",
    "~/plugins/socket.io.client.js",
    "~/plugins/v-viewer.js",
    "~/plugins/json_editor_vue.client.js"
  ],

  // Auto import components: https://go.nuxtjs.dev/config-components
  components: true,

  // Modules for dev and build (recommended): https://go.nuxtjs.dev/config-modules
  buildModules: [
  ],

  // Modules: https://go.nuxtjs.dev/config-modules
  modules: [
    '@nuxt/content'
  ],

  // Build Configuration: https://go.nuxtjs.dev/config-build
  build: {
    // Vite 4 (Rollup 3) uses ES2020 as compiler target by default
    // Therefore Vite-4-built outputs should be transpiled in webpack 4
    transpile: ['json-editor-vue'],
    extend (config, { isDev, isClient }) {

      config.node= {
         fs: 'empty',
         child_process: "empty"
       }

      // Getting webpack to recognize the `.mjs` file
      config.module.rules.push({
        test: /\.mjs$/,
        include: /node_modules/,
        type: 'javascript/auto',
      })
    },
    loaders: {
      sass: {
        sassOptions: {
          quietDeps: true
        }
      }
    }
  },

  publicRuntimeConfig: {
    macworp_base_url: process.env.MACWORP_BACKEND_BASE_URL || 'http://localhost:3001',
    macworp_ws_url: process.env.MACWORP_BACKEND_WS_URL || 'http://localhost:3001',
    macworp_upload_max_file_size: process.env.MACWORP_UPLOAD_MAX_FILE_SIZE || 5368709120, // 5GB
    macworp_render_max_file_size: process.env.MACWORP_RENDER_MAX_FILE_SIZE || 1048576, // 1MB
  },

  server: {
    host: process.env.MACWORP_FRONTEND_INTERFACE || "127.0.0.1",
    port: process.env.MACWORP_FRONTEND_PORT || 5001,
    timing: false
  }
}
