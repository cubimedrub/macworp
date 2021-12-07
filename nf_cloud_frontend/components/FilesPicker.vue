<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <select v-model="current_value" class="form-select" multiple>
                <option v-for="file in available_files" :key="file" :value="file">{{file}}</option>
            </select>
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
        /**
         * Files to choose
         */
        available_files: {
            type: Array,
            required: true
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
        }
    }
}
</script>