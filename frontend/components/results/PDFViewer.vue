<template>
    <div class="pdf-viewer">
        <h2>{{ result_file_header }}</h2> 
        <div v-if="authorized_url" class="d-flex flex-column align-items-center">
            <object type="application/pdf" :data="authorized_url">{{ result_file_not_found_message }}</object>
            <p v-if="result_file_description">{{ result_file_description }}</p>
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
import ResultRendererMixin from '@/mixins/result_renderer'
/**
 * Component to display PDF files.
 */
export default {
    mixins: [
        ResultRendererMixin
    ],
    data(){
        return {
            authorized_url: null,
        }
    },
    mounted(){
        this.getAuthenticatedUrlForRender(this.path).then(url => {
            this.authorized_url = url
        })
    }
}
</script>