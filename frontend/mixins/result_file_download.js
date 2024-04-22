const FILE_NOT_FOUND_MESSAGE = "Result file not ready yet."

/**
 * Status of the result file download.
 */
const RESULT_FILE_DOWNLOAD_STATUS_MAP = {
    FETCHING: "fetching",
    NOT_FOUND: "not_found",
    FINISHED: "finished"
}

/**
 * Mixin for components that will render or downlaod results.
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
            result_file_download_status: RESULT_FILE_DOWNLOAD_STATUS_MAP.FETCHING,
        }
    },
    methods: {
        /**
         * Returns the download URL with a one-time-use token for downloading the file.
         * Necessary for downloads that require authentication as the we can not send the JWT token in the body of a GET request.
         * 
         * @param {String} path Path to file in project directory
         * @returns {Promise} Preauthenticated download URL (can be used in a GET request to download the file)
         */
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
                } else {
                    return this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Downloads the file from the server.
         * 
         * @param {String} path Path to file in project directory
         * @param {Boolean} is_inline Whether to download the file inline or as an attachment
         * @returns {Promise} Promise returning the response
         */
        async downloadFile(path, is_inline) {
            return this.authenticateFileDownload(path).then(download_url => {
                if(is_inline) {
                    download_url += "&inline=true"
                }
                return fetch(download_url).then(response => {
                    if(response.ok) {
                        this.result_file_download_status = RESULT_FILE_DOWNLOAD_STATUS_MAP.FINISHED
                        return response
                    } else if (response.status == 404) {
                        this.result_file_download_status = RESULT_FILE_DOWNLOAD_STATUS_MAP.NOT_FOUND
                        return Promise.reject()
                    } else {
                        return this.handleUnknownResponse(response)
                    }
                })
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
         * Returns the result_file_download_status_map
         */
        result_file_download_status_map() {
            return RESULT_FILE_DOWNLOAD_STATUS_MAP
        }
    }
}