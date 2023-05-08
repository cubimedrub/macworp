<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <div class="input-group">
                <input v-if="!is_multiline" v-model="current_value" :disabled="!enabled" type="text" class="form-control">
                <textarea v-if="is_multiline" v-model="current_value" :rows="textarea_rows" :disabled="!enabled" class="form-control"></textarea>
            </div>
            <small v-if="description != null" class="ms-2">{{description}}</small>
        </div>
    </div>
</template>

<script>
import InputMixin from "../mixins/input"

/**
 * Minimum number of rows for the textarea.
 */
const MIN_TEXTAREA_ROWS = 4

export default {
    mixins: [
        InputMixin
    ],
    props: {
        /**
         * If true, the input is a textfield.
         */
        is_multiline: {
            type: Boolean,
            required: false,
            default: false
        }
    },
    computed: {
        /**
         * Returns the number of rows needed to show the current value without the need od scrolling.
         * At least MIN_TEXTAREA_ROWS is returned.
         * 
         * @returns {number}
         */
        textarea_rows(){
            if(number_of_newlines == null)
                return MIN_TEXTAREA_ROWS
            var number_of_newlines = (this.current_value.match(/\n/, g) || []).length()
            return number_of_newlines >= MIN_TEXTAREA_ROWS ? number_of_newlines + 1 : MIN_TEXTAREA_ROWS
        },
        /** @override */
        value_type(){
            return String
        }
    }
}
</script>