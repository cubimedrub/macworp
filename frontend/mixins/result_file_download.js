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
    methods: {
        /**
         * Returns the download URL with a one-time-use token for downloading the file.
         * Necessary for downloads that require authentication as we can not send the JWT token in the body of a GET request.
         * 
         * @param {String} path Path to file in project directory
         * @returns {Promise} Preauthenticated download URL (can be used in a GET request to download the file)
         */
        async authenticateFileDownload(path) {
            return fetch(`${this.$config.macworp_base_url}/api/users/one-time-use-token`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        return `${this.$config.macworp_base_url}/api/projects/${this.project_id}/download?path=${path}&one-time-use-token=${response_data.token}`
                    })
                } else {
                    return Promise.reject(response)
                }
            })
        },
        /**
         * Downloads the file from the server.
         * 
         * @param {String} path Path to file in project directory
         * @param {Boolean} is_inline Whether to download the file inline or as an attachment
         * @param {Boolean} with_metadata Whether to return the metadata as headers
         * @param {Boolean} is_table Whether the file is a table an should be returned in JSON format
         * @returns {Promise} Promise returning the response
         */
        async downloadFile(path, is_inline, with_metadata, is_table) {
            return this.authenticateFileDownload(path).then(download_url => {
                if (is_inline)
                    download_url = download_url + "&is-inline=1"
                if (with_metadata)
                    download_url = download_url + "&with-metadata=1"
                if (is_table)
                    download_url = download_url + "&is-table=1"

                return fetch(download_url).then(response => {
                    if(response.ok) {
                        return response
                    } else {
                        return Promise.reject(response)
                    }
                })
            })
        },
        /**
         * Download metadata
         * 
         * @param {String} path Path to file in project directory
         */
        async downloadResultFileMetadata(path) {
            return fetch(`${this.$config.macworp_base_url}/api/projects/${this.project_id}/metadata?path=${path}`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        return response_data
                    })
                } else {
                    return Promise.reject(response)
                }
            })
        }
    }
}