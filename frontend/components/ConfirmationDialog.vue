<template>
    <div :ref="identifier" class="modal" tabindex="-1">
        <div class="modal-dialog">
            <div class="modal-content">
                <div class="modal-header">
                    <h5 class="modal-title">
                        <slot name="header"></slot>
                    </h5>
                    <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                </div>
                <div class="modal-body">
                    <slot name="body"></slot>
                </div>
                <div class="modal-footer">
                    <button @click="dismiss" :disabled="is_confirming" type="button" class="btn btn-secondary" data-bs-dismiss="modal">
                        <slot name="dismiss-button"></slot>
                    </button>
                    <button @click="confirm" :class="[confirm_button_class]" :disabled="is_confirming" type="button" class="btn">
                        <slot name="confirm-button"></slot>
                    </button>
                </div>
            </div>
        </div>
    </div>
</template>

<script>
import Vue from "vue"

export default {
    props: {
        /**
         * Local event bus for communication with parent.
         */
        local_event_bus: {
            required: true,
            type: Vue
        },
        /**
         * Callback on confirmation
         */
        on_confirm_func: {
            required: true,
            type: Function
        },
        /**
         * Identifier
         */
        identifier: {
            default: "confirmation-dialog",
            type: String
        },
        /**
         * CSS class confirmation button
         */
        confirm_button_class: {
            default: "btn-primary",
            type: String
        }
    },
    data(){
        return {
            is_confirming: false
        }
    },
    mounted(){
        this.initialize()
    },
    activate(){
        this.initialize()
    },
    methods: {
        /**
         * Initializes instance
         */
        initialize(){
            // Event listener on local event bus for opening if the send identifier
            // is equals to the one passed down with in `props`
            this.local_event_bus.$on("CONFIRMATION_DIALOG_OPEN", identifier => {
                if(identifier == this.identifier) this.show()
            })
        },
        /**
         * Opens the modal.
         */
        show(){
            this.openModal(this.identifier)
        },
        /**
         * Calls callback and closes modal.
         */
        async confirm(){
            if(!this.is_confirming){
                this.is_confirming = true
                await this.on_confirm_func()
                this.is_confirming = false
                this.dismiss()
            }
        },
        /**
         * Closes modal without any action.
         */
        dismiss(){
            this.closeModal(this.identifier)
        }
    }
}
</script>