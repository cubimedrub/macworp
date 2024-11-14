<template>
    <div>
        <div v-if="project && !project_not_found">
            <div class="d-flex justify-content-between align-items-center">
                <div class="d-flex align-items-start">
                    <h1>Project "{{ project.name }}"</h1>
                    <span v-if="project.ignore" class="badge bg-warning text-dark ml-3">Currently ignored</span>
                </div>
                <button @click="showStartDialog" :disabled="project.is_scheduled || !project.workflow_id in workflows || project.ignore" class="btn btn-success btn-sm">
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
            <div v-if="project.is_scheduled" class="mb-3">
                <h2>Progress</h2>
                <div class="progress">
                    <div class="progress-bar bg-success" role="progressbar" :style="progress_bar_style"></div>
                </div>
                <small>
                    <b>Hint:</b> The progress might decrease during the run as number of processes is initially unknown.
                </small>
            </div>
            <div v-if="project.is_scheduled || logs.length > 0" class="mb-3">
                <h2>Logs</h2>
                <div class="mb-1">
                    <textarea v-model="logs_text" ref="logs" class="form-control" id="logs" rows="3" disabled readonly></textarea>
                </div>
                <small>
                    <b>Hint:</b> Logs are not persisted yet. Only the logs received since the page was accessed are displayed.
                </small>
            </div>
            <div v-if="error_report" class="mb-3">
                <h2>Error report</h2>
                <div class="alert alert-danger" role="alert">
                    <pre>{{ error_report }}</pre>
                </div>
                <small>
                    <b>Hint:</b> The error report is not persisted yet. Save it and show it to your trusty developer.
                </small>
            </div>
            <Tab :tabs="tabs" :tab_labels="tab_labels" :preselected_tab="selected_tab" :parent_event_bus="local_event_bus">
                <template v-slot:files>
                    <EditableFileBrowser
                        :project_id="project.id"
                        :parent_event_bus="local_event_bus"
                        :reload_event="reload_project_files_event"
                        :enabled="!project.is_scheduled"
                    ></EditableFileBrowser>
                </template>

                <template v-slot:workflow>
                    <div class="dropdown mb-3">
                        <button :class="{show: show_workflow_dropdown}" :aria_expanded="show_workflow_dropdown" @click="toggleWorkflowDropdown" :disabled="project.is_scheduled" class="btn btn-primary" type="button" id="workflows-dropdown" data-bs-toggle="dropdown">
                            <span v-if="project.workflow_id">{{ workflows[project.workflow_id] }}</span>
                            <span v-else>Select a workflow...</span>
                            <i :class="{'fa-caret-down': !show_workflow_dropdown, 'fa-caret-up': show_workflow_dropdown}" class="fas ms-2"></i>
                        </button>
                        <ul :class="{show: show_workflow_dropdown}" class="dropdown-menu" aria-labelledby="workflows-dropdown">
                            <li v-for="(workflow_name, workflow_id) in workflows" :key="workflow_id">
                                <button @click="setWorkflow(workflow_id); toggleWorkflowDropdown();" :disabled="project.is_scheduled" type="button" class="btn btn-link text-decoration-none text-body">
                                    {{workflow_name}}
                                </button>
                            </li>
                        </ul>

                    </div>
                    <div v-if="selected_workflow_description">
                        <h2 class="mb-0">Workflow description</h2>
                        <div  v-html="selected_workflow_description" class="mb-3"></div>
                    </div>

                    <h2 class="mb-0">Workflow parameters</h2>
                    <p v-if="project.workflow_id == 0">
                        No workflow selected. Please select a workflow from the dropdown above.
                    </p>
                    <template v-for="(argument, argument_idx) in project.workflow_arguments">
                        <PathSelector
                            v-if="argument.type == 'path'"
                            :name="argument.name"
                            :label="argument.label"
                            :description="argument.desc"
                            :initial_value="project.workflow_arguments[argument_idx].value"
                            :parent_event_bus="local_event_bus"
                            :value_change_event="argument_changed_event"
                            :enabled="!project.is_scheduled"
                            :project_id="project.id"
                            :with_selectable_files="argument.selectable_files"
                            :with_selectable_folders="argument.selectable_folders"
                            :key="argument_idx"
                        ></PathSelector>
                        <MultiplePathSelector
                            v-if="argument.type == 'paths'"
                            :name="argument.name"
                            :label="argument.label"
                            :description="argument.desc"
                            :initial_value="project.workflow_arguments[argument_idx].value || []"
                            :parent_event_bus="local_event_bus"
                            :value_change_event="argument_changed_event"
                            :enabled="!project.is_scheduled"
                            :available_files="project.files"
                            :project_id="project.id"
                            :with_selectable_files="argument.selectable_files"
                            :with_selectable_folders="argument.selectable_folders"
                            :key="argument_idx"
                        ></MultiplePathSelector>
                        <TextInput v-if="argument.type == 'text'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="project.workflow_arguments[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :enabled="!project.is_scheduled" :is_multiline="argument.is_multiline" :key="argument_idx"></TextInput>
                        <NumberInput v-if="argument.type == 'number'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="project.workflow_arguments[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :enabled="!project.is_scheduled" :key="argument_idx"></NumberInput>
                        <FileGlob v-if="argument.type == 'file-glob'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="project.workflow_arguments[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :enabled="!project.is_scheduled" :key="argument_idx"></FileGlob>
                        <ValueSelect v-if="argument.type == 'value-select'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="project.workflow_arguments[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="argument_changed_event" :enabled="!project.is_scheduled" :options="argument.options" :is_multiselect="argument.is_multiselect" :key="argument_idx"></ValueSelect>
                        <Separator v-if="argument.type == 'separator'" :label="argument.label" :key="argument_idx"></Separator>
                    </template>
                    <button @click="updateProject" :disabled="project.is_scheduled" type="button" class="btn btn-primary mb-3">
                        <i class="fas fa-save me-2"></i>
                        save
                    </button>
                </template>

                <template v-slot:results>
                    <div class="results-container">
                        <SelectableFileBrowser 
                            :project_id="project.id"
                            :parent_event_bus="local_event_bus"
                            :directory_change_event="result_dir_change_event"
                            :with_selectable_files="false"
                            :with_selectable_folders="false"
                            :reload_event="reload_project_files_event"
                            :enabled="true"
                        ></SelectableFileBrowser>

                        <template v-for="filepath in result_dir_file_paths">
                            <ResultsImageViewer
                                v-if="isImageFile(filepath)"
                                :project_id="project.id"
                                :path="filepath"
                                :key="filepath"
                            ></ResultsImageViewer>
                            <ResultsPDFViewer
                                v-if="filepath.endsWith('.pdf')"
                                :project_id="project.id"
                                :path="filepath"
                                :key="filepath"
                            ></ResultsPDFViewer>
                            <!-- Wraps the SVG in an image tag -->
                            <ResultsSVGViewer
                                v-if="filepath.endsWith('.image.svg')"
                                :project_id="project.id"
                                :path="filepath"
                                :embed="false"
                                :key="filepath"
                            ></ResultsSVGViewer>
                            <!-- Adds the SVG into the DOM -->
                            <ResultsSVGViewer
                                v-if="filepath.endsWith('.svg') && !filepath.endsWith('.image.svg')"
                                :project_id="project.id"
                                :path="filepath"
                                :embed="true"
                                :key="filepath"
                            ></ResultsSVGViewer>
                            <ResultsPlotlyViewer
                                v-if="filepath.endsWith('.plotly.json')"
                                :project_id="project.id"
                                :path="filepath"
                                :key="filepath"
                            ></ResultsPlotlyViewer>
                            <ResultsTableView
                                v-if="isTableFile(filepath)"
                                :project_id="project.id"
                                :path="filepath"
                                :key="filepath"
                            ></ResultsTableView>
                            <ResultsTextViewer
                                v-if="filepath.endsWith('txt')"
                                :project_id="project.id"
                                :path="filepath"
                                :key="filepath"
                            ></ResultsTextViewer>
                        </template>
                    </div>
                </template>
            </Tab>
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
import toastr from "toastr"

const RELOAD_WORKFLOW_FILES_EVENT = "RELOAD_WORKFLOW_FILES"
const DELETE_CONFIRMATION_DIALOG_ID = "delete_confirmation_dialog"
const START_WORKFLOW_CONFIRMATION_DIALOG_ID = "start_workflow_confirmation_dialog"

/**
 * Keys for tabs
 */
const TABS = ['files', 'workflow', 'results'];
/**
 * Labels for tabs
 */
const TAB_LABELS = ['Files', 'Workflow', 'Results'];
/**
 * Event name for argument changes.
 */
const ARGUMENT_CHANGED_EVENT = "ARGUMENT_CHANGED"

/**
 * Event name for result dir change.
 */
const RESULT_DIR_CHANGE_EVENT = "RESULT_DIR_CHANGE"

/** 
 * Table file extensions
 */
const TABLE_FILE_EXTENSIONS = [".csv", ".tsv"]

/** 
 * Image file extensions
 */
const IMAGE_FILE_EXTENSIONS = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]

export default {
    data(){
        return {
            project: null,
            upload_queue: [],
            is_uploading: false,
            project_not_found: false,
            workflows: {},
            selected_workflow_description: "",
            show_workflow_dropdown: false,
            /**
             * Event bus for communication with child components.
             */
            local_event_bus: new Vue(),
            logs: [],
            error_report: null,
            selected_tab: 0,
            result_dir_file_paths: []
        }
    },
    mounted(){
        this.loadProject()
        this.getWorkflows()
        this.bindWorkflowArgumentChangeEvent()
        this.bindResultDirChangeEvent()
        // Set selected tab and bind event for tab changes
        this.selected_tab = this.$route.query.tab ? TABS.indexOf(this.$route.query.tab) : 0
        this.local_event_bus.$on("TAB_CHANGED", (tab_idx) => {
            this.selected_tab = tab_idx
        })
    },
    deactivated(){
        this.disconnectFromProjectSocketIoRoom()
    },
    methods: {
        loadProject(){
            this.disconnectFromProjectSocketIoRoom()
            return fetch(
                `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok) {
                    response.json().then(response_data => {
                        let project = response_data.project
                        this.project = project
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
                `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}/delete`,
                {
                    method: "POST",
                    headers: {
                        "x-access-token": this.$store.state.login.jwt
                    }
                }
            ).then(response => {
                if(response.ok || response.status == 404) {
                    this.$router.push({name: "projects"})
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        startProject(){
            if(this.project.is_scheduled) return
            if(this.project.workflow_id in this.workflows){
                fetch(
                    `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}/schedule`,
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
                            toastr.success("Workflow is scheduled for execution")
                        })
                    } else {
                        if (response.status == 409) {
                            response.json().then(data => {
                                toastr.error(data.errors.general)
                            })
                        } else {
                            this.handleUnknownResponse(response)
                        }
                    }
                })
            }
        },
        updateProject(){
            if(this.project.is_scheduled) return
            fetch(
                `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}/update`,
                {
                    method:'POST',
                    headers: {
                        "Content-Type": "application/json",
                        "x-access-token": this.$store.state.login.jwt
                    },
                    body: JSON.stringify({
                        workflow_arguments: this.project.workflow_arguments,
                        workflow_id: this.project.workflow_id,
                    })
                }
            ).then(response => {
                if(response.ok) {
                    toastr.success("Project updated")
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        getWorkflows(){
            fetch(`${this.$config.macworp_base_url}/api/workflows/published`, {
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
        setWorkflow(workflow_id){
            if (this.project.is_scheduled) return
            this.project.workflow_id = workflow_id
            this.getDynamicWorkflowArguments()
        },
        /**
         * Sets a neww value to the workflow argument
         *
         * @param {number} argument_name
         * @param {any} argument_value
         */
        setWorkflowArgument(argument_name, argument_value){
            let argument_index = this.project.workflow_arguments.findIndex(argument => argument.name == argument_name)
            this.project.workflow_arguments[argument_index].value = argument_value
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
            fetch(`${this.$config.macworp_base_url}/api/workflows/${this.project.workflow_id}/arguments`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.project.workflow_arguments = data
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
            this.$socket.on("error", data => {
                this.error_report += `${data.error_report}\n`
            })
            this.$socket.on("finished-project", () => {
                this.project.is_scheduled = false
                this.project.submitted_processes = 0
                this.project.completed_processes = 0
                this.local_event_bus.$emit(this.reload_project_files_event)
            })
            this.$socket.on("new-progress", data => {
                this.project.submitted_processes = data.submitted_processes
                this.project.completed_processes = data.completed_processes
                this.logs.push(data.details)
                this.$nextTick(() => {
                    this.$refs.logs.scrollTop = this.$refs.logs.scrollHeight
                })
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
        /**
         * Fetches the workflow description
         */
        async getWorkflowDescription(){
            if(this.project.workflow_id == 0)
                return Promise.resolve()
            return fetch(`${this.$config.macworp_base_url}/api/workflows/${this.project.workflow_id}/description?parse=1`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.selected_workflow_description = data.description
                    })
                } else {
                    this.handleUnknownResponse(response)
                }
            })
        },
        async onResultDirChange(file_browser_attributes){
            this.result_dir_file_paths = file_browser_attributes.current_directory_files.map(filename => `${file_browser_attributes.current_directory}/${filename}`)
        },
        /**
         * Binds the result dir change event to the local event bus.
         */
        bindResultDirChangeEvent(){
            this.local_event_bus.$on(
                this.result_dir_change_event,
                file_browser_attributes => {this.onResultDirChange(file_browser_attributes)}
            )
        },
        /**
         * Checks if filepath is a table file
         */
        isTableFile(filepath){
            for (const extension of TABLE_FILE_EXTENSIONS){
                if(filepath.endsWith(extension)) return true
            }
            return false
        },
        /**
         * Checks if filepath is an image file
         */
        isImageFile(filepath){
            for (const extension of IMAGE_FILE_EXTENSIONS){
                if(filepath.endsWith(extension)) return true
            }
            return false
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
        },
        /**
         * Returns tabs
         *
         * @returns {string}
         */
        tabs(){
            return TABS
        },
        /**
         * Returns tab labels
         *
         * @returns {string}
         */
        tab_labels(){
            return TAB_LABELS
        },
        /**
         * Returns the logs as text 
         *
         * @returns {string}
         */
        logs_text(){
            return this.logs.join("\n")
        },
        /**
         * Provide access to RESULT_DIR_CHANGE_EVENT in vue instance.
         * @return {string}
         */
        result_dir_change_event(){
            return RESULT_DIR_CHANGE_EVENT
        }
    },
    watch: {
        /**
         * Watch for changes in the project id and reload the project.
         */
        selected_tab(){
            this.$router.replace({
                name: "projects-id",
                params: {
                    id: this.$route.params.id
                },
                query: {
                    tab: this.tabs[this.selected_tab]
                }
            })
        },
        'project.workflow_id': function() {
            this.getWorkflowDescription()
        }
    }
}
</script>

