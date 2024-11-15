<template>
    <div :ref="dialog_identifier" :class="{'show': is_open}" class="modal modal-xl fade" tabindex="-1">
        <div class="modal-dialog">
            <div class="modal-content">
                <div class="modal-header">
                    <h1 class="modal-title fs-5">Start a workflow</h1>
                    <button @click="close()" type="button" class="btn-close" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <div class="dropdown mb-3">
                        <button :class="{show: show_workflow_dropdown}" :aria_expanded="show_workflow_dropdown" @click="toggleWorkflowDropdown" :disabled="project.is_scheduled" class="btn btn-primary" type="button" id="workflows-dropdown" data-bs-toggle="dropdown">
                            <span v-if="selected_workflow_id">{{ workflows[selected_workflow_id] }}</span>
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
                    

                    <div v-if="selected_workflow_id != 0">
                        <div v-if="selected_workflow_description">
                            <h2 class="mb-0">Workflow description</h2>
                            <div v-html="selected_workflow_description" class="mb-3"></div>
                        </div>  

                        <h2 class="mb-0">Workflow parameters</h2>
                        <p v-if="selected_workflow_parameters.length == 0">
                            This workflow has no parameters.
                        </p>

                        <div v-if="errors.general" class="alert alert-primary" role="alert">
                            {{ errors.general }}
                        </div>

                        <template v-for="(argument, argument_idx) in selected_workflow_parameters">
                            <PathSelector
                                v-if="argument.type == 'path'"
                                :name="argument.name"
                                :label="argument.label"
                                :description="argument.desc"
                                :initial_value="selected_workflow_parameters[argument_idx].value"
                                :parent_event_bus="local_event_bus"
                                :value_change_event="parameter_changed_event"
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
                                :initial_value="selected_workflow_parameters[argument_idx].value || []"
                                :parent_event_bus="local_event_bus"
                                :value_change_event="parameter_changed_event"
                                :enabled="!project.is_scheduled"
                                :available_files="project.files"
                                :project_id="project.id"
                                :with_selectable_files="argument.selectable_files"
                                :with_selectable_folders="argument.selectable_folders"
                                :key="argument_idx"
                            ></MultiplePathSelector>
                            <TextInput v-if="argument.type == 'text'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="selected_workflow_parameters[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="parameter_changed_event" :enabled="!project.is_scheduled" :is_multiline="argument.is_multiline" :key="argument_idx"></TextInput>
                            <NumberInput v-if="argument.type == 'number'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="selected_workflow_parameters[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="parameter_changed_event" :enabled="!project.is_scheduled" :key="argument_idx"></NumberInput>
                            <FileGlob v-if="argument.type == 'file-glob'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="selected_workflow_parameters[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="parameter_changed_event" :enabled="!project.is_scheduled" :key="argument_idx"></FileGlob>
                            <ValueSelect v-if="argument.type == 'value-select'" :name="argument.name" :label="argument.label" :description="argument.desc" :initial_value="selected_workflow_parameters[argument_idx].value" :parent_event_bus="local_event_bus" :value_change_event="parameter_changed_event" :enabled="!project.is_scheduled" :options="argument.options" :is_multiselect="argument.is_multiselect" :key="argument_idx"></ValueSelect>
                            <Separator v-if="argument.type == 'separator'" :label="argument.label" :key="argument_idx"></Separator>
                        </template>
                    </div>
                </div>
                <div class="modal-footer">
                    <button @click="close" type="button" class="btn btn-secondary">Close</button>
                    <button @click="startWorkflow" type="button" class="btn btn-primary">
                        <i class="fas fa-play me-2"></i>
                        Start workflow
                    </button>
                </div>
            </div>
        </div>
    </div>
</template>

<script>
import Vue from "vue"
import toastr from "toastr"

// Default event to open the modal
export const DEFAULT_OPEN_EVENT = 'open-workflow-modal'

// Workflow scheduled event
export const WORKFLOW_SCHEDULED_EVENT = 'workflow-scheduled'

// Parameter changed event
export const PARAMETER_CHANGED_EVENT = "workflow-parameter-changed"

export default {
    props: {
        /**
         * Dialog identifier, used to open/close the dialog
         * Change is only needed if this dialog is used multiple times on the same page
         */
        dialog_identifier: {
            type: String,
            default: 'workflow-dialog'
        },
        /**
         * Project object
         */
        project: {
            type: Object,
            required: true
        },
        /**
         * Parent event bus to communicate with the parent component
         */
        parent_event_bus: {
            type: Object,
            required: true
        },
        /**
         * Event to open the modal
         * Change is only needed if this dialog is used multiple times on the same page
         */
        open_event: {
            type: String,
            default: DEFAULT_OPEN_EVENT
        },
    },
    data: function(){
        return {
            // Rendering
            is_open: false,
            show_workflow_dropdown: false,
            // Misc
            local_event_bus: new Vue(),
            // Data
            workflows: [],
            selected_workflow_id: 0,
            selected_workflow_description: '',
            selected_workflow_parameters: {},
            // errors
            errors: {}
        }
    },
    created(){
        this.parent_event_bus.$on(this.open_event, () => {
            this.is_open = true
        }),
        this.bindWorkflowArgumentChangeEvent()
    },
    methods: {
        /**
         * 
         */
        open(){
            this.is_open = true
        },
        /**
         * Closes the modal
         */
        close(){
            this.is_open = false
        },
        /**
         * Fetches the list of published workflows
         */
        async getWorkflows(){
            return fetch(`${this.$config.macworp_base_url}/api/workflows/published`).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.workflows = data.workflows
                    })
                } else {
                    this.close()
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Open/closes the workflow selection dropdown
         */
        toggleWorkflowDropdown(){
            this.show_workflow_dropdown = !this.show_workflow_dropdown
        },
        /**
         * Sets the workflow for the project
         * 
         * @param {number} workflow_id 
         */
        setWorkflow(workflow_id){
            this.selected_workflow_id = workflow_id
        },
        /**
         * Resets the dialog
         */
        reset(){
            this.selected_workflow_id = 0
            this.selected_workflow_description = ''
            this.selected_workflow_parameters = {}
        },
        /**
         * Fetches the dynamic workflow parameters from MAcWorP
         * If workflow was executed before, it will fetch the cached parameters
         */
        async getWorkflowParameters(){
            return fetch(`${this.$config.macworp_base_url}/api/workflows/${this.selected_workflow_id}/parameters`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(workflow_parameters => {
                        this.getCachedWorkflowParameters().then(cached_params => {
                            workflow_parameters.forEach(parameter => {
                                if(parameter.name in cached_params){
                                    parameter.value = cached_params[parameter.name]
                                }
                            })
                        }).then(() => {
                            this.selected_workflow_parameters = workflow_parameters
                        })
                    })
                } else {
                    this.close()
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Fetches the workflow description
         */
        async getWorkflowDescription(){
            if(this.selected_workflow_id == 0) return
            return fetch(`${this.$config.macworp_base_url}/api/workflows/${this.selected_workflow_id}/description?parse=1`, {
            }).then(response => {
                if(response.ok) {
                    response.json().then(data => {
                        this.selected_workflow_description = data.description
                    })
                } else {
                    this.close()
                    this.handleUnknownResponse(response)
                }
            })
        },
        /**
         * Starts the workflows
         */
        async startWorkflow(){
            if(this.project.is_scheduled) return
            return fetch(
                `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}/schedule/${this.selected_workflow_id}`,
                {
                    method:'POST',
                    headers: {
                        "x-access-token": this.$store.state.login.jwt,
                        "Content-Type": "application/json"
                    },
                    body: JSON.stringify(this.selected_workflow_parameters)
                }
            ).then(response => {
                if(response.ok) {
                    return response.json().then(response_data => {
                        this.close()
                        this.reset()
                        this.parent_event_bus.$emit(WORKFLOW_SCHEDULED_EVENT, response_data)
                        toastr.success("Workflow is scheduled for execution")
                    })
                } else {
                    switch(response.status){
                        case 409:
                            response.json().then(data => {
                                toastr.error(data.errors.general)
                            })
                            break
                        case 422:
                            response.json().then(data => {
                                if("general" in data.errors){
                                    // clone
                                    this.errors = {
                                        general: `${data.errors.general}`
                                    }
                                    delete data.errors.general
                                }
                                if(Object.keys(data.errors).length > 0){
                                    let msg = Object.keys(data.errors).map(key => {
                                        return `* ${key}: ${data.errors[key]}`
                                    }).join("<br>")
                                    toastr.error(msg)
                                }
                            })
                            break
                        default:
                            this.close()
                            this.handleUnknownResponse(response)
                    }
                }
            })
        },
        /**
         * Sets a new value to the workflow argument
         *
         * @param {number} parameter_name
         * @param {any} parameter_value
         */
        setWorkflowParameter(parameter_name, parameter_value){
            let argument_index = this.selected_workflow_parameters.findIndex(argument => argument.name == parameter_name)
            this.selected_workflow_parameters[argument_index].value = parameter_value
        },
        /**
         * Binds the argument change event to the local event bus.
         */
        bindWorkflowArgumentChangeEvent(){
            this.local_event_bus.$on(
                this.parameter_changed_event,
                (parameter_name, new_value) => {this.setWorkflowParameter(parameter_name, new_value)}
            )
        },
        /**
         * Fetches the cached workflow parameters
         */
        async getCachedWorkflowParameters(){
            return fetch(
                `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}/cached-workflow-parameters/${this.selected_workflow_id}`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt,
                    }
                }
            ).then(response => {
                if(response.ok) {
                    return response.json().then(data => {
                        return data
                    })
                } else {
                    switch(response.status){
                        case 422:
                            return {}
                        default:
                            this.close()
                            this.handleUnknownResponse(response)
                    }
                }
            })
        },
        /**
         * Fetches the last executed workflow
         */
        async getLastExecutedWorkflow(){
            return fetch(
                `${this.$config.macworp_base_url}/api/projects/${this.$route.params.id}/last-executed-workflow`,
                {
                    headers: {
                        "x-access-token": this.$store.state.login.jwt,
                    }
                }
            ).then(response => {
                if(response.ok) {
                    return response.json().then(data => {
                        return data.id
                    })
                } else {
                    switch(response.status){
                        case 422:
                            return 0
                        default:
                            this.close()
                            this.handleUnknownResponse(response)
                    }
                }
            })
        },
    },
    watch: {
        /**
         * Fetches the workflows when the dialog is opened
         * and the last executed workflow if any
         * 
         * @param new_value 
         */
        is_open: function(new_value){
            if(new_value){
                this.openModal(this.dialog_identifier)
                this.getWorkflows().then(() => {
                    this.getLastExecutedWorkflow().then(last_executed_workflow_id => {
                        this.selected_workflow_id = last_executed_workflow_id
                    })
                })
            } else {
                this.closeModal(this.dialog_identifier)
            }
        },
        selected_workflow_id: function() {
            if (this.selected_workflow_id != 0) {
                this.getWorkflowParameters()
                this.getWorkflowDescription()
            }
            
        }
    },
    computed: {
        /**
         * Returns the event to trigger when a parameter is changed
         */
        parameter_changed_event(){
            return PARAMETER_CHANGED_EVENT
        }
    }

}
</script>
