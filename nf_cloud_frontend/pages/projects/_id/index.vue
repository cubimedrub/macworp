<template>
    <div>
        <div v-if="project && !project_not_found">
            <div class="d-flex justify-content-between align-items-center">
                <h1>Project "{{ project.name }}"</h1>
                <button @click="showStartDialog" :disabled="project.is_scheduled || !workflows.includes(project.workflow)" class="btn btn-success btn-sm">
                    Start project
                    <i class="fas fa-play"></i>
                </button>
            </div>
            <table class="table">
                <tbody>
                    <tr>
                        <th>ID</th>
                        <td>{{  project.id }}</td>
                    </tr>
                    <tr>
                        <th>Name</th>
                        <td>{{  project.name }}</td>
                    </tr>
                </tbody>
            </table>
            <div class="d-flex justify-content-end">
                <button @click="showDeleteDialog" type="button" class="btn btn-danger">
                    <i class="fas fa-trash"></i>
                    delete
                </button>
            </div>
            <div v-if="project.is_scheduled">
                <h2>Progress</h2>
                <div class="progress">
                    <div class="progress-bar bg-success" role="progressbar" :style="progress_bar_style"></div>
                </div>
            </div>
            <h2>Workflow</h2>
            <div class="dropdown mb-3">
                <button :class="{show: show_workflow_dropdown}" :aria_expanded="show_workflow_dropdown" @click="toggleWorkflowDropdown" class="btn btn-primary" type="button" id="workflows-dropdown" data-bs-toggle="dropdown">
                    <span v-if="project.workflow">{{project.workflow}}</span>
                    <span v-else>Select a project...</span>
                    <i :class="{'fa-caret-down': !show_workflow_dropdown, 'fa-caret-up': show_workflow_dropdown}" class="fas ms-2"></i>
                </button>
                <ul :class="{show: show_workflow_dropdown}" class="dropdown-menu" aria-labelledby="workflows-dropdown">
                    <li v-for="nf_project in workflows" :key="nf_project" :value="nf_project">
                        <button @click="setWorkflow(nf_project); toggleWorkflowDropdown();" type="button" class="btn btn-link text-decoration-none text-body">
                            {{nf_project}}
                        </button>
                    </li>
                </ul>
                
            </div>
<!-- Tab start 
            <ul class="nav nav-pills mb-3" id="pills-tab" role="tablist">
                <li class="nav-item" role="presentation">
                    <button class="nav-link active" id="pills-home-tab" data-bs-toggle="pill" data-bs-target="#pills-home" type="button" role="tab" aria-controls="pills-home" aria-selected="true">Parameter</button>
                </li> 
                <li class="nav-item" role="presentation">
                    <button class="nav-link active" id="pills-profile-tab" data-bs-toggle="pill" data-bs-target="#pills-profile" type="button" role="tab" aria-controls="pills-profile" aria-selected="true">Results</button>
                </li>
                <li class="nav-item" role="presentation">
                    <button class="nav-link" id="pills-contact-tab" data-bs-toggle="pill" data-bs-target="#pills-contact" type="button" role="tab" aria-controls="pills-contact" aria-selected="false">Contact</button>
                </li>
            </ul>
            <div class="tab-content" id="pills-tabContent">
                <div class="tab-pane fade show active" id="pills-home" role="tabpanel" aria-labelledby="pills-home-tab">-->
                    <h2 class="mb-0">Workflow parameters</h2>
                    <template v-for="(argument_value, argument_name) in project.workflow_arguments">
                        <div :key="argument_name">
                            <PathSelector 
                                v-if="argument_value.type == 'path'" 
                                :label="argument_name" 
                                :description="argument_value.desc"
                                :initial_value="project.workflow_arguments[argument_name].value"
                                :parent_event_bus="local_event_bus"
                                :value_change_event="argument_changed_event"
                                :project_id="project.id"
                                :with_selectable_files="argument_value.selectable_files"
                                :with_selectable_folders="argument_value.selectable_folders"
                            ></PathSelector>
                            <MultiplePathSelector 
                                v-if="argument_value.type == 'paths'" 
                                :label="argument_name"
                                :description="argument_value.desc"
                                :initial_value="project.workflow_arguments[argument_name].value"
                                :parent_event_bus="local_event_bus"
                                :value_change_event="argument_changed_event" 
                                :available_files="project.files"
                                :project_id="project.id"
                                :with_selectable_files="argument_value.selectable_files"
                                :with_selectable_folders="argument_value.selectable_folders"
                            ></MultiplePathSelector>
                            <TextInput v-if="argument_value.type == 'text'" :label="argument_name" :description="argument_value.desc" :initial_value="project.workflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :is_multiline="project.workflow_arguments[argument_name].is_multiline"></TextInput>
                            <NumberInput v-if="argument_value.type == 'number'" :label="argument_name" :description="argument_value.desc" :initial_value="project.workflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event"></NumberInput>
                            <FileGlob v-if="argument_value.type == 'file-glob'" :label="argument_name" :description="argument_value.desc" :initial_value="project.workflow_arguments[argument_name].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event"></FileGlob>
                        </div>
                    </template>
                <!-- </div>
                <div class="tab-pane fade" id="pills-profile" role="tabpanel" aria-labelledby="pills-profile-tab">
                    Results:
                </div>
                <div class="tab-pane fade" id="pills-contact" role="tabpanel" aria-labelledby="pills-contact-tab">
                    ...
                </div> 
            </div>-->                                     
<!-- Tab ende -->
<!--tab test
<ul class="nav nav-pills mb-3" id="pills-tab" role="tablist">
  <li class="nav-item" role="presentation">
    <button class="nav-link active" id="pills-home-tab" data-bs-toggle="pill" data-bs-target="#pills-home" type="button" role="tab" aria-controls="pills-home" aria-selected="true">Home</button>
  </li>
  <li class="nav-item" role="presentation">
    <button class="nav-link" id="pills-profile-tab" data-bs-toggle="pill" data-bs-target="#pills-profile" type="button" role="tab" aria-controls="pills-profile" aria-selected="false">Profile</button>
  </li>
  <li class="nav-item" role="presentation">
    <button class="nav-link" id="pills-contact-tab" data-bs-toggle="pill" data-bs-target="#pills-contact" type="button" role="tab" aria-controls="pills-contact" aria-selected="false">Contact</button>
  </li>
</ul>
<div class="tab-content" id="pills-tabContent">
  <div class="tab-pane fade show active" id="pills-home" role="tabpanel" aria-labelledby="pills-home-tab">1</div>
  <div class="tab-pane fade" id="pills-profile" role="tabpanel" aria-labelledby="pills-profile-tab">12</div>
  <div class="tab-pane fade" id="pills-contact" role="tabpanel" aria-labelledby="pills-contact-tab">123</div>
</div>-->

<!-- tab test ende-->
            <button @click="updateProject" type="button" class="btn btn-primary mb-3">
                <i class="fas fa-save me-2"></i>
                save
            </button>
            <h2>Files</h2>
            <EditableFileBrowser 
                :project_id="project.id"
                :parent_event_bus="local_event_bus"
                :reload_event="this.reload_project_files_event"
            ></EditableFileBrowser>
        </div>
        <div v-if="!project && project_not_found">
            Project not found
        </div>
        <ConfirmationDialog :local_event_bus="local_event_bus" :on_confirm_func="deleteProject" :identifier="delete_confirmation_dialog_id" confirm_button_class="btn-danger">
            <template v-slot:header>
                Delete this project?
            </template>
            <template v-slot:body>
                Are you sure you want to delete this project?
            </template>
            <template v-slot:dismiss-button>
                Dismiss
            </template>
            <template v-slot:confirm-button>
                Delete
            </template>
        </ConfirmationDialog>
        <ConfirmationDialog :local_event_bus="local_event_bus" :on_confirm_func="startProject" :identifier="start_workflow_confirmation_dialog_id" confirm_button_class="btn-success">
            <template v-slot:header>
                Start the selected workflow on this project?
            </template>
            <template v-slot:body>
                Are you sure you want to start the workflow?
            </template>
            <template v-slot:dismiss-button>
                Dismiss
            </template>
            <template v-slot:confirm-button>
                Start
            </template>
        </ConfirmationDialog>
    </div>
</template>

<script>
import Vue from "vue"

const RELOAD_WORKFLOW_FILES_EVENT = "RELOAD_WORKFLOW_FILES"
const DELETE_CONFIRMATION_DIALOG_ID = "delete_confirmation_dialog"
const START_WORKFLOW_CONFIRMATION_DIALOG_ID = "start_workflow_confirmation_dialog"
/**
 * Event name for argument changes.
 */
const ARGUMENT_CHANGED_EVENT = "ARGUMENT_CHANGED"

export default {
    data(){
        return {
            project: null,
            upload_queue: [],
            is_uploading: false,
            project_not_found: false,
            workflows: [],
            show_workflow_dropdown: false,
            /**
             * Event bus for communication with child components.
             */
            local_event_bus: new Vue(),
            logs: [],
        }
    },
    mounted(){
        this.loadProject()
        this.getWorkflows()
        this.bindWorkflowArgumentChangeEvent()
    },
    deactivated(){
        this.disconnectFromProjectSocketIoRoom()
    },
    methods: {
        loadProject(){
            this.disconnectFromProjectSocketIoRoom()
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.$route.params.id}`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        this.project = response_data.project
                        this.bindWorkflowArgumentChangeEvent()
                        this.connectToProjectSocketIoRoom()
                    })
                } else if(response.status == 404) {
                    this.project_not_found = true
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        deleteProject(){
            return fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.$route.params.id}/delete`, 
                {
                    method: "POST",
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok || response.status == 404) {
                    this.$router.push({name: "projects"})
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        startProject(){
            if(this.workflows.includes(this.project.workflow)){
                fetch(
                    `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.$route.params.id}/schedule`, 
                    {
                        method:'POST',
                        headers: {
                            "x-access-token": this.$store.state.login.jwt
                        }
                    }
                ).then(response => {
                    if(response.ok) {
                        return response.json().then(response_data => {
                            this.project.is_scheduled = response_data.is_scheduled
                        })
                    } else {
                        this.handleUnknownResponse(response)
                    }
                })
            }
        },
        updateProject(){
            fetch(
                `${this.$config.nf_cloud_backend_base_url}/api/projects/${this.$route.params.id}/update`, 
                {
                    method:'POST',
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        workflow_arguments: this.project.workflow_arguments,
                        workflow: this.project.workflow,
                    })
                }
            ).then(response => {
                if(!response.ok) {
                    this.handleUnknownResponse(response)
                }
            })
        },
        getWorkflows(){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.workflows = data.workflows
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        toggleWorkflowDropdown(){
            this.show_workflow_dropdown = !this.show_workflow_dropdown
        },
        setWorkflow(workflow){
            this.project.workflow = workflow
            this.getDynamicWorkflowArguments()
        },
        /**
         * Sets a nee value to the workflow argument
         * 
         * @param {string} argument_name
         * @param {any} argument_value
         */
        setWorkflowArgument(argument_name, argument_value){
            this.project.workflow_arguments[argument_name].value = argument_value
        },
        /**
         * Binds the argument change event to the local event bus.
         */
        bindWorkflowArgumentChangeEvent(){
            this.local_event_bus.$on(
                this.argument_changed_event, 
                (argument_name, new_value) => {this.setWorkflowArgument(argument_name, new_value)}
            )
        },
        /**
         * Fetches the dynamic workflow arguments from the NFCloud
         * and assigns it to the project.
         */
        getDynamicWorkflowArguments(){
            fetch(`${this.$config.nf_cloud_backend_base_url}/api/workflows/${this.project.workflow}/arguments`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.project.workflow_arguments = data.arguments
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Connect to project room
         */
        connectToProjectSocketIoRoom(){
            this.$socket.emit("join_project_updates", {
                "project_id": this.project.id,
                "access_token": this.$store.state.login.jwt
            })
            this.$socket.on("new-workflow-log", (new_log) => {
                this.project.workflow_log = new_log
                this.logs.push(new_log)
            })
            this.$socket.on("finished-project", () => {
                this.project.is_scheduled = false
                this.project.submitted_processes = 0
                this.project.completed_processes = 0
                this.local_event_bus.$emit(this.reload_project_files_event)
            })
            this.$socket.on("new-progress", data => {
                console.error(data)
                this.project.submitted_processes = data.submitted_processes
                this.project.completed_processes = data.completed_processes
            })
        },
        /**
         * Disconnect from project room
         */
        disconnectFromProjectSocketIoRoom(){
            if(this.project != null) this.$socket.emit("leave_project_updates", {
                "project_id": this.project.id
            });
        },
        /**
         * Opens the delete confirmation dialog
         */
        showDeleteDialog(){
            this.local_event_bus.$emit("CONFIRMATION_DIALOG_OPEN", this.delete_confirmation_dialog_id)
        },
        /**
         * Opens the start confirmation dialog
         */
        showStartDialog(){
            this.local_event_bus.$emit("CONFIRMATION_DIALOG_OPEN", this.start_workflow_confirmation_dialog_id)
        },
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
            var progress = this.project.completed_processes / this.project.submitted_processes * 100
            return `width: ${progress}%`
        },
        /**
         * Provide access to RELOAD_WORKFLOW_FILES_EVENT in vue instance.
         * @return {string}
         */
        reload_project_files_event(){
            return RELOAD_WORKFLOW_FILES_EVENT
        },
        /**
         * Provide access to DELETE_CONFIRMATION_DIALOG_ID in vue instance.
         * @return {string}
         */
        delete_confirmation_dialog_id() {
            return DELETE_CONFIRMATION_DIALOG_ID
        },
        /**
         * Provide access to START_WORKFLOW_CONFIRMATION_DIALOG_ID in vue instance.
         * @return {string}
         */
        start_workflow_confirmation_dialog_id() {
            return START_WORKFLOW_CONFIRMATION_DIALOG_ID
        }
    }
}
</script>

