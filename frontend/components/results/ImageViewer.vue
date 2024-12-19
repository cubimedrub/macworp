<template>
    <div class="image-viewer">
        <h2>{{ result_file_header }}</h2>
        <p v-if="result_file_description">{{ result_file_description }}</p>
        <div v-if="result_file_download_status == result_file_download_status_map.FINISHED" class="d-flex flex-column align-items-center mt-2">
            <v-viewer 
                :images="[authorized_url]"
            >
                <img 
                    :key="authorized_url"
                    :src="authorized_url"
                    :data-source="authorized_url"
                    :alt="result_file_description"
                    @inited="inited"
                    class="image"
                />
            </v-viewer>
        </div>
        
        <div v-if="result_file_download_status == result_file_download_status_map.FETCHING" class="d-flex justify-content-center">
            <Spinner></Spinner>
        </div>
        <div v-if="result_file_download_status == result_file_download_status_map.NOT_FOUND">
            <p>
                {{ result_file_not_found_message }}
            </p>
        </div>
        <div v-if="result_file_download_status == result_file_download_status_map.FILESIZE_TOO_LARGE">
            <p>
                {{ result_file_too_large_message }}
            </p>
        </div>
    </div>
</template>
<script>


import 'viewerjs/dist/viewer.css'
import ResultRendererMixin from '@/mixins/result_renderer'

/**
 * Render a single image
 */
export default {
    mixins: [
        ResultRendererMixin
    ],
    data(){
        return {
            viewer: null,
            authorized_url: null
        }
    },
    mounted(){
        this.getAuthenticatedUrlForRender(this.path).then(url => {
            this.authorized_url = url
        })
    },
    methods: {
        inited(viewer) {
            this.viewer = viewer
        }
    }
}
</script>