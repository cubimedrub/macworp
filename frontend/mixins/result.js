import Vue from "vue"

const FILE_NOT_FOUND_MESSAGE = "Result file not ready yet."

/**
 * Mixin for components that will render results.
 */
export default {
    props: {
        project_id: {
            type: Number,
            required: true
        },
    },
    data() {
        return {
            // Indicates whether the result file is loading.
            result_file_loading: true,
            // Indicates whether the result file is not found.
            internal_result_file_not_found: false,
        }
    },
    methods: {
        async authenticateFileDownload(path) {
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/users/one-time-use-token`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        return `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/download?path=${path}&one-time-use-token=${response_data.token}`
                    })
                } else if (response.status == 404) {
                    this.internal_result_file_not_found = true
                    return Promise.resolve(null)
                } else {
                    return this.handleUnknownResponse(response)
                }
            }).finally(() => {
                this.result_file_loading = false
            })
        }
    },
    computed: {
        /**
         * Returns a common message for result file not found.
         */
        result_file_not_found_message() {
            return FILE_NOT_FOUND_MESSAGE
        },
        /**
         * Indicates the download is finished and the result file is found.
         * 
         */
        result_file_found() {
            return !this.result_file_loading && !this.internal_result_file_not_found
        },
        /**
         * Indicates the download is finished but the result file is not found.
         */
        result_file_not_found() {
            return !this.result_file_loading && !this.internal_result_file_not_found
        }
    }
}