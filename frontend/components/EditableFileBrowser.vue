<template>
    <div class="broder border-dark">
        <div class="row">
            <div class="col-12 col-md-6 col-lg-4 offset-md-6 offset-lg-8">
                <div class="input-group mb-3">
                    <input v-model="new_folder_path" v-on:keyup.enter="createNewFolder" :readonly="is_creating_folder" type="text" class="form-control" placeholder="new folder">
                    <button @click="createNewFolder" :disabled="is_creating_folder || is_new_folder_path_empty" class="btn btn-primary" type="button">
                        <i class="fas fa-plus"></i>
                        create
                    </button>
                </div>
            </div>
        </div>
        <ul class="list-group mb-3">
            <li v-if="current_directory != ''" @click="moveFolderUp()" class="list-group-item">
                <i class="fas fa-angle-double-left clickable"></i>
            </li>
            <li v-for="path in current_directory_folders" :key="path" class="list-group-item d-flex justify-content-between">
                <span @click="moveIntoFolder(path)" class="clickable">
                    <i class="fas fa-folder"></i>
                    {{path}}
                </span>
                <div class="btn-group">
                    <button @click="download(path)" type="button" class="btn btn-secondary btn-sm">
                        <i class="fas fa-download"></i>
                    </button>
                    <button @click="deletePath(`${current_directory}/${path}`)" type="button" class="btn btn-danger btn-sm">
                        <i class="fas fa-trash"></i>
                    </button>
                </div>
            </li>
            <li v-for="file in current_directory_files" :key="file" class="list-group-item d-flex justify-content-between">
                <span>{{ file }}</span>
                <div class="btn-group">
                    <button @click="download(file)" type="button" class="btn btn-secondary btn-sm">
                        <i class="fas fa-download"></i>
                    </button>
                    <button @click="deletePath(`${current_directory}/${file}`)" type="button" class="btn btn-danger btn-sm">
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
            is_creating_folder: false
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
         */
        deletePath(path){
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
                    var path_name = this.getLastSegementOfPath(path)
                    if(path_name.charAt(path_name.length - 1) == "/")
                        this.current_directory_folders = this.current_directory_folders.filter(folder => folder != path_name);
                    else
                        this.current_directory_files = this.current_directory_files.filter(file => file != path_name);
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        createNewFolder(){
            /**
             * Creates a new folder within the current directory.
             */
            if(!this.is_creating_folder){
                this.is_creating_folder = true
                fetch(`${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/create-folder`, {
                    method:'POST',
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        "new_path": this.new_folder_path,
                        "target_path": this.current_directory
                    })
                }).then(response => {
                    if(response.ok) {
                        var new_folder_name = this.getFirstSegementOfPath(this.new_folder_path)
                        // Adding a slash to declarate name as path
                        new_folder_name = new_folder_name.charAt(new_folder_name.length - 1) == "/" ? new_folder_name : `${new_folder_name}/`
                        this.current_directory_folders.push(new_folder_name)
                        this.current_directory_folders.sort()
                    } else {
                        this.handleUnknownResponse(response)
                    }
                }).finally(() => {
                    this.is_creating_folder = false
                    this.new_folder_path = null
                })
            }
        },
        getFirstSegementOfPath(path){
            /**
             * Returns the first segement of the given path.
             * 
             * @param  {String} path    File or folder path.
             * @return {String}         First segment of path.
             */
            // Remove preceding slash if there is one
            var temp_path = path.indexOf("/") == 0 ? path.slice(1, path.length) : path
            // Get first occurence of slash
            var end_of_first_segment = temp_path.indexOf("/")
            // If no slash was found set end_of_first_segment to end of string
            end_of_first_segment = end_of_first_segment >= 0 ? end_of_first_segment : temp_path.length
            return temp_path.slice(0, end_of_first_segment)
        },
        getLastSegementOfPath(path){
            /**
             * Returns the last segement of the given path (without preceeding slash)
             * 
             * @param  {String} path    File or folder path.
             * @return {String}         First segment of path.
             */
            // Remove appending slash if there is one
            var temp_path = path.lastIndexOf("/") == path.length - 1 ? path.slice(0, path.length - 1) : path
            var start_of_last_segment = temp_path.lastIndexOf("/")
            return path.slice(start_of_last_segment + 1, path.length)
        },
        /**
         * Downloads the given path.
         *
         * @param {String} path Path to file in relation to the project directory
         */
        async download(path){
            let complete_path = `${this.current_directory}${path}`
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/users/one-time-use-token`, {
                headers: {
                    "x-access-token": this.$store.state.login.jwt
                }
            }).then(response => {
                console.error(response.status)
                if(response.ok) {
                    console.error(response.status)
                    return response.json().then(response_data => {
                        window.location = `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.project_id}/download?path=${complete_path}&one-time-use-token=${response_data.token}`
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        }
    },
    computed: {
        /**
         * Checks if new_folder_path is empty
         * 
         * @return {Boolean}
         */
        is_new_folder_path_empty(){
            return this.new_folder_path == null
                || this.new_folder_path.trim().length == 0
        }
    }
}
</script>