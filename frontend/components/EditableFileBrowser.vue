<template>
    <div class="broder border-dark">
        <div class="row">
            <div class="col-12 col-md-6 col-lg-4 offset-md-6 offset-lg-8">
                <div class="input-group mb-3">
                    <input v-model="new_folder_name" v-on:keyup.enter="createNewFolder" type="text" class="form-control" placeholder="new folder">
                    <button @click="createNewFolder" :disabled="is_new_folder_name_empty" class="btn btn-primary" type="button">
                        <i class="fas fa-plus"></i>
                        create
                    </button>
                </div>
            </div>
        </div>
        <ul class="list-group mb-3">
            <li v-if="!is_current_directory_root" @click="moveFolderUp()" class="list-group-item">
                <i class="fas fa-angle-double-left clickable"></i>
            </li>
            <li v-for="path in current_directory_folders" :key="path" class="list-group-item d-flex justify-content-between">
                <span @click="moveIntoFolder(path)" class="clickable">
                    <i class="fas fa-folder"></i>
                    {{path}}/
                </span>
                <div class="btn-group">
                    <button @click="download(`${current_directory}/${path}`)" type="button" class="btn btn-secondary btn-sm">
                        <i class="fas fa-download"></i>
                    </button>
                    <button @click="deletePath(`${current_directory}/${path}`, false)" :disabled="!enabled" type="button" class="btn btn-danger btn-sm">
                        <i class="fas fa-trash"></i>
                    </button>
                </div>
            </li>
            <li v-for="file in current_directory_files" :key="file" class="list-group-item d-flex justify-content-between">
                <span>{{ file }}</span>
                <div class="btn-group">
                    <button @click="download(`${current_directory}/${file}`)" type="button" class="btn btn-secondary btn-sm">
                        <i class="fas fa-download"></i>
                    </button>
                    <button @click="deletePath(`${current_directory}/${file}`, true)" :disabled="!enabled" type="button" class="btn btn-danger btn-sm">
                        <i class="fas fa-trash"></i>
                    </button>
                </div>
            </li>
        </ul>
        <div ref="dropzone" class="dropzone mb-3"></div>
        <div v-if="upload_queue.length">
            <h3>Upload queue</h3>
            <ul class="list-group">
                <li v-for="upload_item in upload_queue" :key="upload_item" class="list-group-item d-flex justify-content-between">
                    <span>{{ upload_item }}</span>
                    <div class="w-25 d-flex justify-content-end align-items-center">
                        <div v-if="upload_status[upload_item] > 0" class="progress w-100">
                            <div :style="`width: ${upload_status[upload_item]}%`" :aria-valuenow="upload_status[upload_item]" class="progress-bar" role="progressbar"  aria-valuemin="0" aria-valuemax="100"></div>
                        </div>
                        <span v-else>wait for upload...</span>
                    </div>
                </li>
            </ul>
        </div>
    </div>
</template>

<script>
import Dropzone from "dropzone";
import ProjectFileBrowser from '../mixins/project_file_browser'

const DIRECTORY_REFRESH_DELAY_MS = 200

export default {
    /**
     * Implements logic for navigating through the project work directory with 
     * the ability to create new directories, upload new files and delete present files.
     */
    mixins: [
        ProjectFileBrowser
    ],
    data(){
        return {
            // logic
            new_folder_name: null,
            // Queue & order of uploads
            upload_queue: [],
            // Upload status
            upload_status: {},
            /**
             * Directory refresh timeout after upload to prevent overwhelming the server with requests
             * when uploading many small files where each upload only need sa few milliseconds.
             */ 
            directory_refresh_timeout: null,
            // elements
            dropzone: null
        }
    },
    mounted(){
        this.initDropzone()
    },
    methods: {
        /**
         * Initializes the dropzone for file uploads.
         */
        initDropzone() {
            this.dropzone = new Dropzone(
                this.$refs.dropzone, 
                { 
                    url: `${this.$config.macworp_base_url}/api/projects/${this.project_id}/upload-file-chunk?is-dropzone=1`,
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    },
                    clickable: false,
                    disablePreviews: true,
                    parallelUploads: 1,
                    maxFilesize: this.$config.macworp_upload_max_file_size,
                    chunking: true,
                    retryChunks: true,
                    chunkSize: 100000000 // 100MB
                }
            );
            this.dropzone.on("addedfile", file => {
                var original_path = file.fullPath == null ? file.name : file.fullPath
                file.macworp__file_path = this.getFullPath(original_path)
                this.upload_queue.push(file.macworp__file_path)
                this.upload_status[file.macworp__file_path] = 0
            }),
            this.dropzone.on("sending", (file, xhr, formData) => {
                var file_path_blob = new Blob([file.macworp__file_path], { type: "text/plain"})
                formData.append("file_path", file_path_blob)
            });
            this.dropzone.on("uploadprogress", (file, progress) => {
                this.upload_status[file.macworp__file_path] = Math.round(progress * 100) / 100
                this.$forceUpdate()
            });
            this.dropzone.on("success", file => {
                this.upload_queue = this.upload_queue.filter(path => path != file.macworp__file_path)
                delete this.upload_status[file.macworp__file_path]

                // set timeout for refreshing the directory content
                if(this.directory_refresh_timeout == null) {
                    this.directory_refresh_timeout = setTimeout( 
                        () => {
                            this.getFolderContent()
                            this.directory_refresh_timeout = null
                        },
                        DIRECTORY_REFRESH_DELAY_MS
                    )
                }
                
            });
        },
        /**
         * Deletes a path. If path ending with slash it is a directory.
         * 
         * @param {String} path
         * @param {Boolean} is_file If true the path belongs to a file, otherwise to a directory.
         */
        deletePath(path, is_file){
            if(!this.enabled) return
            fetch(`${this.$config.macworp_base_url}/api/projects/${this.project_id}/delete-path`, {
                method:'POST',
                headers: {
                    "Content-Type": "application/json",
                    "x-access-token": this.$store.state.login.jwt
                },
                body: JSON.stringify({
                    "path": path
                })
            }).then(response => {
                if(response.ok) {
                    var last_segment = this.getLastSegmentOfPath(path)
                    if(is_file)
                        this.current_directory_files = this.current_directory_files.filter(file => file != last_segment);   
                    else
                        this.current_directory_folders = this.current_directory_folders.filter(folder => folder != last_segment);
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Creates a new folder within the current directory.
         */
        async createNewFolder(){
            if(this.is_new_folder_name_empty) return
            var target_folder = `${this.current_directory}`
            var new_folder_name = `${this.new_folder_name}`
            this.new_folder_name = null
            var new_folder_path = `${target_folder}/${new_folder_name}`
            fetch(`${this.$config.macworp_base_url}/api/projects/${this.project_id}/create-folder`, {
                method:'POST',
                headers: {
                    "Content-Type": "application/json",
                    "x-access-token": this.$store.state.login.jwt
                },
                body: JSON.stringify({
                    path: new_folder_path
                })
            }).then(response => {
                if(response.ok) {
                    // If user did not navigate into a new folder after the request was startet add the new folder
                    if(this.current_directory == target_folder && !this.current_directory_folders.includes(this.new_folder_name)) {
                        this.current_directory_folders.push(new_folder_name)
                        this.current_directory_folders.sort()
                    }
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Returns the last segment of the given path (without preceeding slash)
         * 
         * @param  {String} path    File or folder path.
         * @return {String}         Last segment of path.
         */
        getLastSegmentOfPath(path){
            if(path.length == 0 || path == "/") {
                return "/"
            }

            var parts = path.split("/")
            parts = parts.filter(part => part.length > 0)
            
            return `${parts[parts.length - 1]}`
        },
        /**
         * Downloads the given path.
         *
         * @param {String} path Path to file in relation to the project directory
         */
        async download(path){
            return fetch(`${this.$config.macworp_base_url}/api/users/one-time-use-token`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        window.location = `${this.$config.macworp_base_url}/api/projects/${this.project_id}/download?path=${path}&one-time-use-token=${response_data.token}`
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
    },
    computed: {
        /**
         * Checks if new_folder_path is empty
         * 
         * @return {Boolean}
         */
        is_new_folder_name_empty(){
            return this.new_folder_name == null
                || this.new_folder_name.trim().length == 0
        },
        is_current_directory_root(){
            return this.current_directory == "/"
        }
    }
}
</script>