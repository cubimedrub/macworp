import BootstrapModal from "bootstrap/js/dist/modal"
import Vue from 'vue'

Vue.mixin({
    methods: {
        /**
         * Opens a bootstrap 5 modal
         * 
         * @param {String} modal_ref ref arrtibute of the modal
         */
        openModal(modal_ref){ 
            // Add attribute `ref` with modal ID to modal element
            var modal = new BootstrapModal(this.$refs[modal_ref], {
                keyboard: false
            })
            modal.show()
        },
        /**
         * Closes a bootstrap 5 modal
         * 
         * @param {String} modal_ref ref arrtibute of the modal
         */
        closeModal(modal_ref){
            var modal = BootstrapModal.getInstance(this.$refs[modal_ref])
            if (modal != null) {
                modal.hide()
            }
        }
    }
})