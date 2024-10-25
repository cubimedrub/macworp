<template>
    <div>
        <h2>{{ result_file_header }}</h2>
        <p v-if="result_file_description">{{ result_file_description }}</p>
        <div v-if="result_file_download_status == result_file_download_status_map.FINISHED">
            <div class="d-flex flex-column align-items-center">
                <textarea :rows="rows" v-model="txt" class="form-control" readonly></textarea>
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
 * Component to display a text file
 */
export default {
    mixins: [
        ResultRendererMixin
    ],
    data(){
        return {
            txt: ""
        }
    },
    mounted(){
        this.downloadFileForRender(this.path, true, false).then( response => {
            response.text().then(txt => {
                this.txt = txt
            })
        })
    },
    computed: {
        rows(){
            if (this.txt == ""){
                return 1
            }
            return this.txt.split("\n").length
        }
    }
}
</script>