<template>
    <div class="pdf-viewer">
        <h2>{{ header }}</h2> 
        <div v-if="authorized_url" class="d-flex flex-column align-items-center">
            <object type="application/pdf" :data="authorized_url"></object>
            <p>{{ description }}</p>
        </div>
    </div>
</template>

<script>
import ResultMixin from '../mixins/result.js'

/**
 * Component to display PDF files.
 */
export default {
    mixins: [
        ResultMixin
    ],
    props: {

        /**
         * Path to the PDF.
         */
        path: {
            type: String,
            required: true
        },
        /**
         * Header of the PDF.
         */
        header: {
            type: String,
            required: true
        },
        /**
         * Description of the PDF.
         */
        description: {
            type: String,
            required: true
        }
    },
    data(){
        return {
            authorized_url: null,
        }
    },
    beforeMount(){
        this.authenticateFileDownload(this.path).then(url => {
            // needs to bed downloaded without attachment header => is-inline=1
            this.authorized_url = `${url}&is-inline=1`
        })
    }
}
</script>