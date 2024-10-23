import Vue from "vue"

export default {
    /**
     * Implements logic for navigating through the project work directory.
     */
    props: {
        project_id: {
            type: Number,
            required: true
        },
        /**
         * Event bus for communication with parent
         */
        parent_event_bus: {
            type: Vue,
            required: true
        },
        /**
         * Event for reloading folder content.
         */
        reload_event: {
            type: String,
            required: false,
            default: "RELOAD"
        },
        /**
         * Active status, if false some inputs are disabled
         */
        enabled: {
            type: Boolean,
            required: true
        },
        /**
         * Event which sends the current directory path and content over the parent event bus
         */
        directory_change_event: {
            type: String,
            required: false,
            default: null
        }
    },
    data(){
        return {
            current_directory: "/",
            current_directory_folders: [],
            current_directory_files: []
        }
    },
    mounted(){
        this.getFolderContent()
        this.parent_event_bus.$on(this.reload_event, () => { this.getFolderContent() })
        this.sendDirectoryChange()
    },
    activated(){
        this.getFolderContent()
    },
    methods: {
        moveFolderUp(){
            let path_segments = this.current_directory.split("/").filter(segment => segment.length > 0)
            path_segments.pop()
            this.current_directory = path_segments.length > 0 ? `/${path_segments.join("/")}` : "/"
        },
        moveIntoFolder(path){
            let path_segments = this.current_directory.split("/").filter(segment => segment.length > 0)
            path_segments.push(path)
            this.current_directory = path_segments.length > 0 ? `/${path_segments.join("/")}` : "/"
        },
        /**
         * Sends the current directory path and content over the parent event bus
         */
        sendDirectoryChange(){
            if (this.directory_change_event != null){
                this.parent_event_bus.$emit(this.directory_change_event, {
                    "current_directory": this.current_directory,
                    "current_directory_folders": this.current_directory_folders,
                    "current_directory_files": this.current_directory_files,
                })
            }
        },
        getFolderContent(){
            var url_encoded_path = encodeURIComponent(this.current_directory)
            fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/files?dir=${url_encoded_path}`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        this.current_directory_folders = response_data.folders
                        this.current_directory_files = response_data.files
                        this.sendDirectoryChange()
                    })
                } else if(response.status == 404) {
                    /**
                     * If folder was not found, move back into parent directory
                     */
                    this.moveFolderUp()
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Concatenates the given path with the current path
         * 
         * @param {String} path 
         * @returns {String}
         */
        getFullPath(path){
            if (this.current_directory === "/"){
                return `${this.current_directory}${path}`
            }
            return `${this.current_directory}/${path}`
        }
    },
    watch: {
        current_directory(){
            this.getFolderContent()
        }
    }
}