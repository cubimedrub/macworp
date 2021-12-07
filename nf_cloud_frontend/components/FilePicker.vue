<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <div class="input-group">
                <select v-model="current_value" class="form-select">
                    <option :value="null" disabled>Select a file...</option>
                    <option v-for="file in available_files" :key="file" :value="file">{{file}}</option>
                </select>
                <button @click="removeFileSelection" class="btn btn-danger" type="button">
                    <i class="fas fa-times"></i>
                </button>
            </div>
            <small v-if="description != null" class="ms-2">{{description}}</small>
        </div>
    </div>
</template>

<script>
import InputMixin from "../mixins/input"

export default {
    mixins: [
        InputMixin
    ],
    props: {
        available_files: {
            type: Array,
            required: true
        }
    },
    methods: {
        /**
         * Sets the current value to null.
         */
        removeFileSelection(){
            this.current_value = null
        },
    },
    computed: {
        /** 
         * @override 
         */
        value_type(){
            return String
        }
    }
}
</script>