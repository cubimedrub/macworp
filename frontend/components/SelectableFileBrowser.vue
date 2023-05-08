<template>
    <div class="broder border-dark">
        <ul class="list-group mb-3">
            <li v-if="current_directory != ''" @click="moveFolderUp()" class="list-group-item">
                <i class="fas fa-angle-double-left clickable"></i>
            </li>
            <li v-for="folder in current_directory_folders" :key="folder" class="list-group-item d-flex justify-content-between">
                <span @click="moveIntoFolder(folder)" class="clickable">
                    <i class="fas fa-folder"></i>
                    {{folder}}
                </span>
                <button v-if="with_selectable_folders" @click="selectPath(getFullPath(folder))" :disabled="!enabled" type="button" class="btn btn-outline-primary btn-sm">
                    <i class="fas fa-angle-right"></i>
                </button>
            </li>
            <li v-for="file in current_directory_files" :key="file" class="list-group-item d-flex justify-content-between">
                <span>{{ file }}</span>
                <button v-if="with_selectable_files" @click="selectPath(getFullPath(file))" :disabled="!enabled" type="button" class="btn btn-outline-primary btn-sm">
                    <i class="fas fa-angle-right"></i>
                </button>
            </li>
        </ul>
    </div>
</template>


<script>
import Vue from 'vue'
import ProjectFileBrowser from "../mixins/project_file_browser"

export default {
    /**
     * Implements logic for navigating through the project work directory with 
     * the ability to select a path.
     */
    mixins: [
        ProjectFileBrowser
    ],
    props: {
        /**
         * Event for selecting a file
         */
        select_event: {
            type: String,
            required: false,
            default: "SELECTED_PATH"
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
        },
    },
    methods: {
        selectPath(path){
            /**
             * Path to file or folder
             * 
             * @param  {String} path    File or folder path.
             */
            if (!this.enabled) return
            this.parent_event_bus.$emit(this.select_event, path)
        },
    }
}
</script>