export default {
  // Target: https://go.nuxtjs.dev/config-target
  target: 'server',
  // Disable server side rendering, so that middleware is executed on page refresh and route changes.
  ssr: false,

  // Global page headers: https://go.nuxtjs.dev/config-head
  head: {
    title: 'NF Cloud',
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
   }
  },

  publicRuntimeConfig: {
    nf_cloud_backend_base_url: process.env.NF_CLOUD_BACKEND_BASE_URL || 'http://localhost:3001',
    nf_cloud_backend_ws_url: process.env.NF_CLOUD_BACKEND_WS_URL || 'http://localhost:3001',
    nf_cloud_upload_max_file_size: process.env.NF_CLOUD_UPLOAD_MAX_FILE_SITE || 5368709120, // 5GB
    nf_cloud_render_max_file_size: process.env.NF_CLOUD_RENDER_MAX_FILE_SIZE || 1048576, // 1MB
  },

  server: {
    host: process.env.NF_CLOUD_FRONTEND_INTERFACE || "127.0.0.1",
    port: process.env.NF_CLOUD_FRONTEND_PORT || 5001,
    timing: false
  }
}
