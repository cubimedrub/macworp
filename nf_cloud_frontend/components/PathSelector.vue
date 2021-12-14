<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <div class="input-group">
                <input v-model="current_value" v-on:keyup.enter="createNewFolder" type="text" class="form-control">
                <button @click="openModal(modal_ref); reloadFolderContent();" class="btn btn-primary" type="button">
                    <i class="fas fa-folder"></i>
                </button>
                <button @click="removeFileSelection" class="btn btn-danger" type="button">
                    <i class="fas fa-times"></i>
                </button>
            </div>
            <small v-if="description != null" class="ms-2">{{description}}</small>
        </div>
        <div :ref="modal_ref" class="modal fade" tabindex="-1">
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 v-if="with_selectable_files && with_selectable_folders" class="modal-title">Select file or folder</h5>
                        <h5 v-if="with_selectable_files && !with_selectable_folders" class="modal-title">Select file</h5>
                        <h5 v-if="!with_selectable_files && with_selectable_folders" class="modal-title">Select folder</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                    </div>
                    <div class="modal-body">
                        <SelectableFileBrowser 
                            :workflow_id="workflow_id"
                            :parent_event_bus="local_event_bus"
                            :select_event="value_change_event"
                            :with_selectable_files="with_selectable_files"
                            :with_selectable_folders="with_selectable_folders"
                            :reload_event="reload_folder_content_event"
                        ></SelectableFileBrowser>
                    </div>
                    <div class="modal-footer">
                        <button type="button" class="btn btn-secondary" data-bs-dismiss="modal">Close</button>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>

<script>
import Vue from "vue"
import InputMixin from "../mixins/input"

/**
 * Event for reloading the folder content
 */
const RELOAD_FOLDER_CONTENT_EVENT = "RELOAD_FOLDER_CONTENT"

export default {
    mixins: [
        InputMixin
    ],
    props: {
        /**
         * Workflow ID
         */
        workflow_id: {
            type: Number,
            required: true
        },
        /**
         * Overwrite if multiple file picker are set one page.
         */
        modal_ref: {
            type: String,
            required: false,
            default: "path-selector-modal"
        },
        /**
         * Makes files selectable
         */
        with_selectable_files: {
            type: Boolean,
            required: false,
            default: false
        },
        /**
         * Makes folders selectable
         */
        with_selectable_folders: {
            type: Boolean,
            required: false,
            default: false
        }
    },
    data(){
        return {
            local_event_bus: new Vue()
        }
    },
    mounted(){
        this.local_event_bus.$on(this.value_change_event, new_value => this.setNewValueAndCloseModal(new_value))
    },
    methods: {
        setNewValueAndCloseModal(new_value){
            this.current_value = new_value
            this.closeModal(this.modal_ref)
        },
        removeFileSelection(){
            this.current_value = null
        },
        reloadFolderContent(){
            this.local_event_bus.$emit(this.reload_folder_content_event)
        }
    },
    computed: {
        /** 
         * @override 
         */
        value_type(){
            return String
        },
        /**
         * Makes RELOAD_FOLDER_CONTENT_EVENT accessible in vue instance
         * @return {String}
         */
        reload_folder_content_event(){
            return RELOAD_FOLDER_CONTENT_EVENT
        }
    }
}
</script>