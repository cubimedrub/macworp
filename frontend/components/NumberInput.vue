<template>
    <div class="row mb-3">
        <label for="lower-mass-tolerance" class="col-sm-2 col-form-label">{{ label }}</label>
        <div class="col-sm-10 d-flex flex-column justify-content-center">
            <div class="input-group">
                <input v-model.number="current_value" type="number" class="form-control">
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
    computed: {
        /**
         * @override
         */
        value_type(){
            return Number
        }
    }
}
</script>