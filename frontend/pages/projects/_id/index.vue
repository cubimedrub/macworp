<template>
    <div>
        <div v-if="project && !project_not_found">
            <div class="d-flex justify-content-between align-items-center">
                <div class="d-flex align-items-start">
                    <h1>Project "{{ project.name }}"</h1>
                    <span v-if="project.ignore" class="badge bg-warning text-dark ml-3">Currently ignored</span>
                </div>
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
                <button @click="showStartDialog" :disabled="project.is_scheduled || project.ignore" class="btn btn-success mx-3">
                    <i class="fas fa-play"></i>
                    Start workflow
                </button>
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
        <WorkflowDialog
            v-if="project"
            :project="project"
            :parent_event_bus="local_event_bus"
        > 
        </WorkflowDialog>
    </div>
</template>

<script>
import Vue from "vue"

import {DEFAULT_OPEN_EVENT as OPEN_WORKFLOW_DIALOG, WORKFLOW_SCHEDULED_EVENT} from "~/components/WorkflowDialog"

const RELOAD_WORKFLOW_FILES_EVENT = "RELOAD_WORKFLOW_FILES"
const DELETE_CONFIRMATION_DIALOG_ID = "delete_confirmation_dialog"

/**
 * Keys for tabs
 */
const TABS = ['files', 'results'];
/**
 * Labels for tabs
 */
const TAB_LABELS = ['Files', 'Results'];
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
        this.bindResultDirChangeEvent()
        // Set selected tab and bind event for tab changes
        this.selected_tab = this.$route.query.tab ? TABS.indexOf(this.$route.query.tab) : 0
        this.local_event_bus.$on("TAB_CHANGED", (tab_idx) => {
            this.selected_tab = tab_idx
        })
        // Lock project on workflow start
        this.local_event_bus.$on(WORKFLOW_SCHEDULED_EVENT, () => {
            this.project.is_scheduled = true
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
                        // this.bindWorkflowArgumentChangeEvent()
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
            this.local_event_bus.$emit(OPEN_WORKFLOW_DIALOG)
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
        }
    }
}
</script>

