<template>
    <div class="border border-dark">
        <ul class="list-group mb-3">
            <li v-if="current_directory != ''" @click="moveFolderUp()" class="list-group-item">
                <i class="fas fa-angle-double-left clickable"></i>
            </li>
            <li v-for="folder in current_directory_folders" :key="folder" class="list-group-item d-flex justify-content-between">
                <span @click="moveIntoFolder(folder)" class="clickable">
                    <i class="fas fa-folder"></i>
                    {{folder}}
                </span>
                <button v-if="with_selectable_folders && isPathSelected(getFullPath(folder))" @click="unselectPath(getFullPath(folder))" :disabled="!enabled" type="button" class="btn btn-outline-primary btn-sm">
                    <i class="far fa-check-square"></i>
                </button>
                <button v-if="with_selectable_folders && !isPathSelected(getFullPath(folder))" @click="selectPath(getFullPath(folder))" :disabled="!enabled" type="button" class="btn btn-outline-primary btn-sm">
                    <i class="far fa-square"></i>
                </button>
            </li>
            <li v-for="file in current_directory_files" :key="file" class="list-group-item d-flex justify-content-between">
                <span>{{ file }}</span>
                <button v-if="with_selectable_files && isPathSelected(getFullPath(file))" @click="unselectPath(getFullPath(file))" :disabled="!enabled" type="button" class="btn btn-outline-primary btn-sm">
                    <i class="far fa-check-square"></i>
                </button>
                <button v-if="with_selectable_files && !isPathSelected(getFullPath(file))" @click="selectPath(getFullPath(file))" :disabled="!enabled" type="button" class="btn btn-outline-primary btn-sm">
                    <i class="far fa-square"></i>
                </button>
            </li>
        </ul>
    </div>
</template>


<script>
import ProjectFileBrowser from "../mixins/project_file_browser"

export default {
    /**
     * Implements logic for navigating through the project work directory with 
     * the ability to select multiple paths.
     */
    mixins: [
        ProjectFileBrowser
    ],
    props: {
        /**
         * Array of selected paths for marking as selected
         */
        selected_paths: {
            type: Array,
            required: true
        },
        /**
         * Event for selecting a path
         */
        select_event: {
            type: String,
            required: false,
            default: "SELECTED_PATH"
        },
        /**
         * Event for unselecting a path
         */
        unselect_event: {
            type: String,
            required: false,
            default: "UNSELECTED_PATH"
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
    methods: {
        selectPath(path){
            /**
             * Select path
             * 
             * @param  {String} path    File or folder path.
             */
            if(!this.enabled) return
            this.parent_event_bus.$emit(this.select_event, path)
        },
        unselectPath(path){
            /**
             * Unselect path
             * 
             * @param  {String} path    File or folder path.
             */
            if(!this.enabled) return
            this.parent_event_bus.$emit(this.unselect_event, path)
        },
        isPathSelected(path){
            return this.selected_paths.includes(path)
        }
    }
}
</script>