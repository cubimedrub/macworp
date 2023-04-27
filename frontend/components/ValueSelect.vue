<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <div class="input-group">
                <select v-model="current_value" :multiple="is_multiselect" class="form-select">
                    <option 
                        v-if="current_value === null && current_value === undefined && initial_value != undefined && initial_value != null" 
                        :value="initial_value"
                        selected 
                        disabled
                    >Select ...</option>
                    <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
                </select>
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
        /**
         * Array of possible values and labels, formatted as [{value: ..., label: ...}, ...]
         */
        options: {
            type: Array,
            required: true
        },
        is_multiselect: {
            type: Boolean,
            required: false,
            default: false
        },
    },
    computed: {
        /**
         * @override
         */
        value_type(){
            return Object
        }
    }
}
</script>