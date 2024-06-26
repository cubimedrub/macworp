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
                    <button @click="deletePath(`${current_directory}${path}`, false)" :disabled="!enabled" type="button" class="btn btn-danger btn-sm">
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
        <div @click="passClickToFileInput" @drop.prevent="addDroppedFiles" @dragover.prevent class="filedrop-area d-flex justify-content-center align-items-center mb-3">
            <span class="unselectable">
                Drag new files here or click here
                <input @change="addSelectedFiles" ref="file-input" multiple type="file" class="hidden-file-input" />
            </span>
        </div>
        <div v-if="upload_queue.length">
            <h3>Upload queue</h3>
            <ul class="list-group">
                <li v-for="upload_item in upload_queue" :key="upload_item.file.name" class="list-group-item d-flex justify-content-between">
                    <span>{{ upload_item.directory }}/{{ upload_item.file.name }}</span>
                    <div class="w-25 d-flex justify-content-end align-items-center">
                        <Spinner v-if="upload_item.is_uploading"></Spinner>
                        <span v-else>wait for upload...</span>
                    </div>
                </li>
            </ul>
        </div>
    </div>
</template>

<script>
import ProjectFileBrowser from '../mixins/project_file_browser'

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
            // uploads
            is_uploading: false,
            upload_queue: [],
            // logic
            new_folder_path: null,
        }
    },
    methods: {
        passClickToFileInput(){
            this.$refs["file-input"].click()
        },
        addSelectedFiles(event) {
            var selected_files = event.target.files
            if(!selected_files) return
            this.addFiles([...selected_files])
            event.target.value = event.target.defaultValue
        },
        addDroppedFiles(event) {
            var dopped_files = event.dataTransfer.files
            if(!dopped_files) return
            this.addFiles([...dopped_files])
        },
        addFiles(new_files){
            new_files.forEach(new_file => {
                this.upload_queue.push({
                    is_uploading: false,
                    file: new_file,
                    directory: `${this.current_directory}` // force copying current directory by using a template string
                })
            });
            this.uploadNewFiles()
        },
        async uploadNewFiles(){
            if(!this.is_uploading){
                this.is_uploading = true;
                while(this.upload_queue.length > 0){
                    var form_data = new FormData();
                    form_data.append("file", this.upload_queue[0].file);
                    form_data.append("directory", this.upload_queue[0].directory);
                    this.upload_queue[0].is_uploading = true;
                    await fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/upload-file`, {
                        method:'POST',
                        headers: {
                            "x-access-token": this.$store.state.login.jwt
                        },
                        body: form_data
                    }).then(response => {
                        if(response.ok) {
                            return response.json().then(response_data => {
                                if(response_data.directory == this.current_directory && !this.current_directory_files.includes(response_data.file)){
                                    this.current_directory_files.push(response_data.file)
                                    this.current_directory_files.sort()
                                }
                                return Promise.resolve()
                            })
                        } else {
                            this.handleUnknownResponse(response)
                        }
                    }).finally(() => {
                        this.upload_queue.shift()
                        return Promise.resolve()
                    })
                }
                this.is_uploading = false
            }
        },
        /**
         * Deletes a path. If path ending with slash it is a directory.
         * 
         * @param {String} path
         * @param {Boolean} is_file If true the path belongs to a file, otherwise to a directory.
         */
        deletePath(path, is_file){
            if(!this.enabled) return
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/delete-path`, {
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
                    var last_segment = this.getLastSegmentOfPath(path).slice(1) // get last segment without preceding slash
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
            var new_folder_path = `${target_folder}${new_folder_name}`
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/create-folder`, {
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
        getFirstSegmentOfPath(path){
            /**
             * Returns the first segment of the given path.
             * 
             * @param  {String} path    File or folder path.
             * @return {String}         First segment of path (e.g. `/` == root or `/folder` == first folder)
             */

            if(path.length == 0 || path == "/") {
                return "/"
            }

            var parts = path.split("/")
            parts = parts.filter(part => part.length > 0)
            
            return `/${parts[0]}`
        },
        getLastSegmentOfPath(path){
            /**
             * Returns the last segment of the given path (without preceeding slash)
             * 
             * @param  {String} path    File or folder path.
             * @return {String}         Last segment of path.
             */

            if(path.length == 0 || path == "/") {
                return "/"
            }

            var parts = path.split("/")
            parts = parts.filter(part => part.length > 0)

            return `/${parts[parts.length - 1]}`
        },
        /**
         * Downloads the given path.
         *
         * @param {String} path Path to file in relation to the project directory
         */
        async download(path){
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/users/one-time-use-token`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        window.location = `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/download?path=${path}&one-time-use-token=${response_data.token}`
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