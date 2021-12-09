<template>
    <div>
        <div v-if="workflow && !workflow_not_found">
            <div class="d-flex justify-content-between align-items-center">
                <h1>Workflow "{{ workflow.name }}"</h1>
                <button @click="startWorkflow" :disabled="workflow.is_scheduled" class="btn btn-success btn-sm">
                    Start workflow
                    <i class="fas fa-play"></i>
                </button>
            </div>
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
            <div v-if="workflow.is_scheduled">
                <h2>Progress</h2>
                <div class="progress">
                    <div class="progress-bar bg-success" role="progressbar" :style="progress_bar_style"></div>
                </div>
            </div>
            <h2>Nextflow Workflow</h2>
            <div class="dropdown mb-3">
                <button :class="{show: show_nextflow_workflow_dropdown}" :aria_expanded="show_nextflow_workflow_dropdown" @click="toggleNextflowWorkflowDropdown" class="btn btn-primary" type="button" id="nextflow-workflow-dropdown" data-bs-toggle="dropdown">
                    <span v-if="workflow.nextflow_workflow"><i :class="getNextflowTypeIconClass(workflow.nextflow_workflow_type)" class="me-2"></i> {{workflow.nextflow_workflow}}</span>
                    <span v-else>Select a workflow...</span>
                    <i :class="{'fa-caret-down': !show_nextflow_workflow_dropdown, 'fa-caret-up': show_nextflow_workflow_dropdown}" class="fas ms-2"></i>
                </button>
                <ul :class="{show: show_nextflow_workflow_dropdown}" class="dropdown-menu" aria-labelledby="nextflow-workflow-dropdown">
                    <template v-for="(nf_workflows, nf_workflow_type) in nextflow_workflows">
                        <li :key="nf_workflow_type + 'divider'"><h6 class="dropdown-header"><i :class="getNextflowTypeIconClass(nf_workflow_type)" class="me-2"></i>{{getNextflowTypeName(nf_workflow_type)}}</h6></li>
                        <li v-for="nf_workflow in nf_workflows" :key="nf_workflow_type + nf_workflow" :value="nf_workflow">
                            <button @click="setNextflowWorkflow(nf_workflow, nf_workflow_type); toggleNextflowWorkflowDropdown();" type="button" class="btn btn-link text-decoration-none text-body">
                                {{nf_workflow}}
                            </button>
                        </li>
                    </template>
                </ul>
                
            </div>
            <h2 class="mb-0">Nextflow parameters</h2>
            <template v-for="(argument_value, argument_name) in workflow.nextflow_arguments">
                <div :key="argument_name">
                    <FilePicker v-if="argument_value.type == 'file'" :label="argument_name" :description="argument_value.desc" :initial_value="workflow.nextflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :available_files="workflow.files"></FilePicker>
                    <FilesPicker  v-if="argument_value.type == 'files'" :label="argument_name" :description="argument_value.desc" :initial_value="workflow.nextflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :available_files="workflow.files"></FilesPicker>
                    <TextInput v-if="argument_value.type == 'text'" :label="argument_name" :description="argument_value.desc" :initial_value="workflow.nextflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :is_multiline="workflow.nextflow_arguments[argument_name].is_multiline"></TextInput>
                    <NumberInput v-if="argument_value.type == 'number'" :label="argument_name" :description="argument_value.desc" :initial_value="workflow.nextflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event"></NumberInput>
                    <FileGlob v-if="argument_value.type == 'file-glob'" :label="argument_name" :description="argument_value.desc" :initial_value="workflow.nextflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event"></FileGlob>
                </div>
            </template>
            <button @click="updateWorkflow" type="button" class="btn btn-primary mb-3">
                <i class="fas fa-save me-2"></i>
                save
            </button>
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
import Vue from "vue"
import socket from '~/plugins/socket.io.js'


const NEXTFLOW_WORKFLOW_TYPE_ICON_CLASS_MAP = {
    "local": "fas fa-hdd",
    "docker": "fab fa-docker"
}

const NEXTFLOW_WORKFLOW_TYPE_NAME_MAP = {
    "local": "Local",
    "docker": "Docker"
}

/**
 * Event name for argument changes.
 */
const ARGUMENT_CHANGED_EVENT = "ARGUMENT_CHANGED"

export default {
    data(){
        return {
            workflow: null,
            upload_queue: [],
            is_uploading: false,
            workflow_not_found: false,
            nextflow_workflows: {
                local: [],
                docker: {}
            },
            show_nextflow_workflow_dropdown: false,
            /**
             * Event bus for communication with child components.
             */
            local_event_bus: new Vue(),
            logs: []
        }
    },
    mounted(){
        this.loadWorkflow()
        this.getNextflowWorkflows()
        this.bindNextflowArgumentChangeEvent()
    },
    deactivated(){
        this.disconnectFromWorkflowSocketIoRoom()
    },
    methods: {
        loadWorkflow(){
            this.disconnectFromWorkflowSocketIoRoom()
            return fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}`)
            .then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.workflow = response_data.workflow
                        this.bindNextflowArgumentChangeEvent()
                        this.connectToWorkflowSocketIoRoom()
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
        },
        startWorkflow(){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/schedule`, {
                method:'POST',
            }).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        this.workflow.is_scheduled = response_data.is_scheduled
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        updateWorkflow(){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.$route.params.id}/update`, {
                method:'POST',
                headers: {
                    "Content-Type": "application/json"
                },
                body: JSON.stringify({
                    nextflow_arguments: this.workflow.nextflow_arguments,
                    nextflow_workflow: this.workflow.nextflow_workflow,
                    nextflow_workflow_type: this.workflow.nextflow_workflow_type
                })
            }).then(response => {
                if(!response.ok) {
                    this.handleUnknownResponse(response)
                }
            })
        },
        getNextflowWorkflows(){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/nextflow-workflows`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.nextflow_workflows = data.nextflow_workflows
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        toggleNextflowWorkflowDropdown(){
            this.show_nextflow_workflow_dropdown = !this.show_nextflow_workflow_dropdown
        },
        setNextflowWorkflow(nextflow_workflow, nextflow_workflow_type){
            this.workflow.nextflow_workflow = nextflow_workflow
            this.workflow.nextflow_workflow_type = nextflow_workflow_type
            this.getDynamicNextflowArguments()
        },
        getNextflowTypeIconClass(nextflow_workflow_type){
            return NEXTFLOW_WORKFLOW_TYPE_ICON_CLASS_MAP[nextflow_workflow_type]
        },
        getNextflowTypeName(nextflow_workflow_type){
            return NEXTFLOW_WORKFLOW_TYPE_NAME_MAP[nextflow_workflow_type]
        },
        /**
         * Sets a nee value to the nextflow argument
         * 
         * @param {string} argument_name
         * @param {any} argument_value
         */
        setNextflowArgument(argument_name, argument_value){
            this.workflow.nextflow_arguments[argument_name].value = argument_value
        },
        /**
         * Binds the argument change event to the local event bus.
         */
        bindNextflowArgumentChangeEvent(){
            this.local_event_bus.$on(
                this.argument_changed_event, 
                (argument_name, new_value) => {this.setNextflowArgument(argument_name, new_value)}
            )
        },
        /**
         * Fetches the dynamic nextflow arguments from the NFCloud
         * and assigns it to the workflow.
         */
        getDynamicNextflowArguments(){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/nextflow-workflows/${this.workflow.nextflow_workflow_type}/${this.workflow.nextflow_workflow}/arguments`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.workflow.nextflow_arguments = data.arguments
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Connect to workflow room
         */
        connectToWorkflowSocketIoRoom(){
            socket.emit("join", {"room": `workflow${this.workflow.id}`})
            socket.on("new-nextflow-log", (new_log) => {
                this.workflow.nextflow_log = new_log
                this.logs.push(new_log)
            })
            socket.on("finished-workflow", () => {
                this.workflow.is_scheduled = false
                this.workflow.submitted_processes = 0
                this.workflow.completed_processes = 0
            })
            socket.on("new-progress", data => {
                console.error(data)
                this.workflow.submitted_processes = data.submitted_processes
                this.workflow.completed_processes = data.completed_processes
            })
        },
        /**
         * Disconnect from workflow room
         */
        disconnectFromWorkflowSocketIoRoom(){
            if(this.workflow != null) socket.emit("leave", {"room": `workflow${this.workflow.id}`});
        }
    },
    computed: {
        /**
         * Returns the argument change event so it is usable in the template.
         * 
         * @returns {string}
         */
        argument_changed_event(){
            return ARGUMENT_CHANGED_EVENT
        },
        /**
         * Returns value for style attribute of progress bar.
         * @return {string}
         */
        progress_bar_style(){
            var progress = this.workflow.completed_processes / this.workflow.submitted_processes * 100
            return `width: ${progress}%`
        }
    }
}
</script>

