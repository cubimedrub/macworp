import ResultFileDownloadMixin from '@/mixins/result_file_download'
import FileTooLargeError from '@/errors/file_too_large_error'

const FILE_NOT_FOUND_MESSAGE = "Result file not ready yet."
const FILESIZE_TOO_LARGE_MESSAGE = "Filesize too large to display. Please download the file instead."

/**
 * Status of the result file download.
 */
const RESULT_FILE_DOWNLOAD_STATUS_MAP = Object.freeze({
    FETCHING: "fetching",
    NOT_FOUND: "not_found",
    FINISHED: "finished",
    FILESIZE_TOO_LARGE: "filesize_too_large"
})


/**
 * Mixin for components that will render a result file.
 * As each file needs to be downloaded, this mixin extends the ResultFileDownloadMixin.
 * Most of the methods are the same as in the ResultFileDownloadMixin, but with additional
 * checks or functionality to render the files. The result file size is usually checked before downloading anything.
 * When the file size check returns ok, the file is downloaded and the metadata are fetched.
 */
export default {
    mixins: [
        ResultFileDownloadMixin
    ],
    props: {
        path: {
            type: String,
            required: true
        }
    },
    data() {
        return {
            result_file_download_status: RESULT_FILE_DOWNLOAD_STATUS_MAP.FETCHING,
            result_file_header: null,
            result_file_description: null,
        }
    },
    created() {
        this.result_file_header = this.path
    },
    methods: {
        /**
         * Fetches the file size
         * 
         * @param {String} path Path to file in project directory
         * @returns {Promise} Resolved promise if file size is ok, rejected promise with FileTooLargeError if file size is too large
         */
        async getFileSize() {
            return fetch(`${this.$config.macworp_base_url}/api/projects/${this.project_id}/file-size?path=${this.path}`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        if(response_data.size <= this.$config.macworp_render_max_file_size ) {
                            return Promise.resolve()
                        } else {
                            this.result_file_download_status = this.result_file_download_status_map.FILESIZE_TOO_LARGE
                            return Promise.reject(new FileTooLargeError())
                        }
                    })
                } else {
                    return Promise.reject(response)
                }
            })
        },
        /**
         * Downloads a file form the server after checking the file size is ok to render.
         * The response can be processed using the given callback while the function
         * fetches the metadata.
         * 
         * @param {String} path Path to file in project directory
         * @param {Boolean} with_metadata Whether to return the metadata as headers
         * @param {Boolean} is_table Whether the file is a table an should be returned in JSON format
         * @returns {Promise} Promise returning the response
         *
         */
        async downloadFileForRender(path, with_metadata, is_table) {
            this.result_file_download_status = this.result_file_download_status_map.FETCHING
            return this.getFileSize(path).then( () => {
                return this.downloadFile(path, true, with_metadata, is_table).then(response => {
                    this.result_file_download_status = this.result_file_download_status_map.FINISHED
                    if (with_metadata) {
                        this.result_file_header = response.headers.get("MMD-Header") || path
                        this.result_file_description = response.headers.get("MMD-Description")
                    }
                    return Promise.resolve(response)
                })
            }).catch(error => {
                if (error instanceof Response) {
                    switch(error.status){
                        case 404:
                            this.result_file_download_status = this.result_file_download_status_map.NOT_FOUND
                            break
                        default:
                            this.handleUnknownResponse(error)
                    }
                } else if (error instanceof FileTooLargeError) {
                    this.result_file_download_status = this.result_file_download_status_map.FILESIZE_TOO_LARGE
                }
                return Promise.reject()
            })
        },
        /**
         * Returns an authenticated URL for downloading the file after a check of the file size
         * It also fetches the metadata of the file.
         * 
         * @param {String} path Path to file in project directory
         * @returns {Promise} Promise returning the columns and data
         *
         */
        async getAuthenticatedUrlForRender(path) {
            this.result_file_download_status = this.result_file_download_status_map.FETCHING
            return this.getFileSize(path).then( () => {
                return this.authenticateFileDownload(path).then(url => {
                    this.result_file_download_status = this.result_file_download_status_map.FINISHED
                    return url
                })
            }).then(authenticated_url => {
                return this.downloadResultFileMetadata(path).then(data => {
                    this.result_file_header = data.header || path
                    this.result_file_description = data.description
                    return Promise.resolve(authenticated_url)
                }).catch(response => {
                    switch(response.status) {
                        case 404:
                            this.result_file_header = path
                            this.result_file_description = ""
                            return Promise.resolve(authenticated_url)
                        default:
                            return Promise.reject(response)
                    }
                })
            }).catch(error => {
                if (error instanceof Response) {
                    switch(error.status){
                        default:
                            this.handleUnknownResponse(error)
                    }
                } else if (error instanceof FileTooLargeError) {
                    this.result_file_download_status = this.result_file_download_status_map.FILE_TOO_LARGE
                }
            })
        },
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
        },
        /**
         * Returns file too large message
         */
        result_file_too_large_message() {
            return FILESIZE_TOO_LARGE_MESSAGE
        }
    }
}