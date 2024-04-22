<template>
    <div class="svg-viewer">
        <h2>{{ header }}</h2>
        <div v-if="result_file_download_status == result_file_download_status_map.FINISHED">
            <div v-if="authorized_url" class="d-flex flex-column align-items-center">
                <img v-if="!embed" :src="authorized_url" :alt="description" width="50%" />
                <div v-else v-html="svg" class="embedded-svg"></div>
                <p>{{ description }}</p>
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
    </div>
</template>

<script>
import ResultMixin from '../mixins/result_file_download'

/**
 * Component to display SVG files.
 */
export default {
    mixins: [
        ResultMixin
    ],
    props: {
        /**
         * Path to the SVG.
         */
        path: {
            type: String,
            required: true
        },
        /**
         * Header of the SVG.
         */
        header: {
            type: String,
            required: true
        },
        /**
         * Description of the SVG.
         */
        description: {
            type: String,
            required: true
        },
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
            this.downloadFile(this.path).then(response => {
                response.text().then(svg => {
                    this.svg = svg
                })
            })
        } else {
            this.authenticateFileDownload(this.path).then(url => {
                // needs to be set manually as the SVG is downloaded by the browser not the mixin
                this.result_file_download_status = this.result_file_download_status_map.FINISHED
                this.authorized_url = url
            })
        }
    }
}
</script>