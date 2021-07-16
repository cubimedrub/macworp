<template>
    <div>
        <div v-if="workflow && !workflow_not_found">
            <h1>Workflow "{{ workflow.name }}"</h1>
            <table class="table">
                <tbody>
                    <tr>
                        <th>ID</th>
                        <td>{{  workflow.id }}</td>
                    </tr>
                    <tr>
                        <th>Name</th>
                        <td>{{  workflow.name }}</td>
                    </tr>
                </tbody>
            </table>
            <div class="d-flex justify-content-end">
                <button @click="deleteWorkflow" type="button" class="btn btn-danger">
                    <i class="fas fa-trash"></i>
                    delete
                </button>
            </div>
            <h2>Files</h2>
            <div @click="passClickToFileInput" @drop.prevent="addDroppedFiles" @dragover.prevent class="filedrop-area d-flex justify-content-center align-items-center mb-3">
                <span class="unselectable">
                    Drag new files here or click here
                    <input @change="addSelectedFiles" ref="file-input" multiple type="file" class="hidden-file-input" />
                </span>
            </div>
            <ul class="list-group mb-3">
                <li v-for="file in workflow.files" :key="file" class="list-group-item d-flex justify-content-between">
                    <span>{{ file }}</span>
                    <button @click="deleteFile(file)" type="button" class="btn btn-danger btn-sm">
                        <i class="fas fa-trash"></i>
                    </button>
                </li>
            </ul>
            <div v-if="upload_queue.length">
                <h3>Upload queue</h3>
                <ul class="list-group">
                    <li v-for="upload_item in upload_queue" :key="upload_item.file.name" class="list-group-item d-flex justify-content-between">
                        <span>{{ upload_item.file.name }}</span>
                        <div class="w-25 d-flex justify-content-end align-items-center">
                            <Spinner v-if="upload_item.is_uploading"></Spinner>
                            <span v-else>wait for upload...</span>
                        </div>
                    </li>
                </ul>
            </div>
        </div>
        <div v-if="!workflow && workflow_not_found">
            Workflow not found
        </div>
    </div>
</template>

<script>
export default {
    data(){
        return {
            workflow: null,
            upload_queue: [],
            is_uploading: false,
            workflow_not_found: false
        }
    },
    mounted(){
        this.loadWorkflow()
    },
    methods: {
        loadWorkflow(){
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}`)
            .then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.workflow = response_data.workflow
                    })
                } else if(response.status == 404) {
                    this.workflow_not_found = true
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        deleteWorkflow(){
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/delete`, {
                method: "POST"
            }).then(response => {
                if(response.ok || response.status == 404) {
                    this.$router.push({name: "workflows"})
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        addSelectedFiles(event) {
            var selected_files = event.target.files
            if(!selected_files) return
            this.addFiles([...selected_files])
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
                    file: new_file
                })
            });
            this.uploadNewFiles()
        },
        async uploadNewFiles() {
            if(!this.is_uploading){
                this.is_uploading = true;
                while(this.upload_queue.length > 0){
                    var form_data = new FormData();
                    form_data.append("file", this.upload_queue[0].file);
                    this.upload_queue[0].is_uploading = true;
                    await fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/upload-file`, {
                        method:'POST',
                        body: form_data
                    }).then(response => {
                        if(response.ok) {
                            return response.json().then(response_data => {
                                this.workflow.files.push(response_data.file)
                                this.upload_queue.shift()
                                return Promise.resolve()
                            })
                        } else {
                            this.handleUnknownResponse(response)
                        }
                    })
                }
                this.is_uploading = false
            }
        },
        passClickToFileInput(){
            this.$refs["file-input"].click()
        },
        deleteFile(filename){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/delete-file`, {
                method:'POST',
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({
                    "filename": filename
                })
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        this.workflow.files = this.workflow.files.filter(file => file != response_data.file);
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        }
    }
}
</script>

