<template>
  <div class="image-viewer">
    <h2>{{ header }}</h2>
    <div v-if="esult_file_download_status == result_file_download_status_map.FINISHED" class="d-flex flex-column align-items-center mt-2">
      <v-viewer 
        :images="authenticated_urls"
      >
        <img 
          v-for="(image, images_idx) in images" 
          :key="image.path"
          :src="authenticated_urls[images_idx]"
          :data-source="authenticated_urls[images_idx]"
          :alt="image.description"
          @inited="inited"
          class="image"
        />
      </v-viewer>
      <p v-if="images.length == 1">
        {{ images[0].description }}
      </p>
    </div>
    <div v-if="result_file_download_status == result_file_download_status_map.FETCHING" class="d-flex justify-content-center">
        <Spinner></Spinner>
    </div>
    <div v-if="result_file_download_status == result_file_download_status_map.NOT_FOUND">
        <p>
            {{ result_file_not_found_message }}
        </p>
    </div>
  </div>
</template>
<script>
import 'viewerjs/dist/viewer.css'
import ResultMixin from '../mixins/result.js'


/**
 * Renders the given images in a gallery with tools like zoom, rotate, etc.
 * If only on image is given it will render the description below the image.
 * Otherwise the description will be shown below the selected image in the gallery.
 */
export default {
  mixins: [
    ResultMixin
  ],
  props: {
    /**
      * Header of the gallery
      */
    header: {
      type: String,
      required: true
    },
    /**
      * Images, with keys:
      * `path` - image path
      * `description` - image description also used as alt text
      */
    images: {
        type: Array,
        required: true
    },
  },
  data(){
    return {
      viewer: null,
      authenticated_urls: []
    }
  },
  beforeMount(){
    Promise.all(this.images.map(image => {
      return this.authenticateFileDownload(image.path)
    })).then(urls => {
      this.authenticated_urls = urls
    })
  },
  methods: {
    inited(viewer) {
      this.viewer = viewer
    }
  }
}
</script>