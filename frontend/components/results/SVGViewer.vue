<template>
    <div class="svg-viewer">
        <h2>{{ result_file_header }}</h2>
        <p v-if="result_file_description">{{ result_file_description }}</p>
        <div v-if="result_file_download_status == result_file_download_status_map.FINISHED">
            <div class="d-flex flex-column align-items-center">
                <div v-if="embed && svg" v-html="svg" class="embedded-svg"></div>
                <img v-else :src="authorized_url" :alt="result_file_description || ''" width="50%" />
            </div>
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
 * Component to display SVG files.
 */
export default {
    mixins: [
        ResultRendererMixin
    ],
    props: {
        /**
         * Whether to embed the SVG in the page or show it as img tag.
         */
        embed: {
            type: Boolean,
            required: false,
            default: false
        }
    },
    data(){
        return {
            authorized_url: null,
            svg: ""
        }
    },
    beforeMount(){
        if (this.embed) {
            this.downloadFileForRender(this.path, true, false).then( response => {
                response.text().then(svg => {
                    this.svg = svg
                })
            })
        } else {
            this.getAuthenticatedUrlForRender(this.path).then(url => {
                this.authorized_url = url
            })
        }
    }
}
</script>