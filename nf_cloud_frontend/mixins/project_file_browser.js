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
        }
    },
    data(){
        return {
            current_directory: "",
            current_directory_folders: [],
            current_directory_files: []
        }
    },
    mounted(){
        this.getFolderContent()
        this.parent_event_bus.$on(this.reload_event, () => { this.getFolderContent() })
    },
    activated(){
        this.getFolderContent()
    },
    methods: {
        moveFolderUp(){
            let path_segments = this.current_directory.split("/").filter(segment => segment.length > 0)
            path_segments.pop()
            this.current_directory = path_segments.length > 0 ? `${path_segments.join("/")}/` : ""
        },
        moveIntoFolder(path){
            this.current_directory = `${this.current_directory}${path}`
        },
        getFolderContent(){
            var url_encoded_path = encodeURIComponent(this.current_directory)
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/files?dir=${url_encoded_path}`)
            .then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        this.current_directory_folders = response_data.folders.map(folder => `${folder}/`)
                        this.current_directory_files = response_data.files
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
            return `${this.current_directory}${path}`
        }
    },
    watch: {
        current_directory(){
            this.getFolderContent()
        }
    }
}