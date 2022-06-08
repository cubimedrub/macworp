<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <div class="input-group">
                <input v-model="current_value" v-on:keyup.enter="createNewFolder" readonly type="text" class="form-control">
                <button @click="openModal(modal_ref); reloadFolderContent();" class="btn btn-primary" type="button">
                    <i class="fas fa-folder"></i>
                </button>
                <button @click="emptyPathSelection" class="btn btn-danger" type="button">
                    <i class="fas fa-times"></i>
                </button>
            </div>
            <small v-if="description != null" class="ms-2">{{description}}</small>
        </div>
        <div :ref="modal_ref" class="modal fade" tabindex="-1">
            <div class="modal-dialog modal-lg">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 v-if="with_selectable_files && with_selectable_folders" class="modal-title">Select files or folders</h5>
                        <h5 v-if="with_selectable_files && !with_selectable_folders" class="modal-title">Select files</h5>
                        <h5 v-if="!with_selectable_files && with_selectable_folders" class="modal-title">Select folders</h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                    </div>
                    <div class="modal-body">
                        <MultipleSelectableFileBrowser 
                            :selected_paths="current_value"
                            :project_id="project_id"
                            :parent_event_bus="local_event_bus"
                            :select_event="selected_path_event"
                            :unselect_event="unselected_path_event"
                            :with_selectable_files="with_selectable_files" 
                            :with_selectable_folders="with_selectable_folders"
                            :reload_event="reload_folder_content_event"
                        ></MultipleSelectableFileBrowser>
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

const SELECTED_PATH_EVENT = "SELECTED_PATH"
const UNSELECTED_PATH_EVENT = "UNSELECTED_PATH"
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
         * Project ID
         */
        project_id: {
            type: Number,
            required: true
        },
        /**
         * Overwrite if multiple file picker are set one page.
         */
        modal_ref: {
            type: String,
            required: false,
            default: "paths-selector-modal"
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
        this.local_event_bus.$on(this.selected_path_event, path => this.addPath(path))
        this.local_event_bus.$on(this.unselected_path_event, path => this.removePath(path))
    },
    methods: {
        /**
         * Adds a path to the list
         * 
         * @param [String] path
         */
        addPath(path){
            this.current_value.push(path)
        },
        /**
         * Removed a path from the list
         * 
         * @param [String] path_to_remove
         */
        removePath(path_to_remove){
            this.current_value = this.current_value.filter(path => path != path_to_remove);
        },
        emptyPathSelection(){
            this.current_value = []
        },
        reloadFolderContent(){
            this.local_event_bus.$emit(this.reload_folder_content_event)
        }
    },
    computed: {
        /**
         * @override
         */
        default_value(){
            return []
        },
        /**
         * @override
         */
        value_type(){
            return Array
        },
        /**
         * Make path select event accessible in Vue instance
         */
        selected_path_event(){
            return SELECTED_PATH_EVENT
        },
        /**
         * Make path unsselect event accessible in Vue instance
         */
        unselected_path_event(){
            return UNSELECTED_PATH_EVENT
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