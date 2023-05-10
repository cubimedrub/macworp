import Vue from 'vue'

/**
 * Defines a mixin for input components, which sends it values back to the parent
 * over an event bus.
 */
export default {
    props: {
        /**
         * Argument name
         */
        name: {
            type: String,
            required: true
        },
        /**
         * Argument name
         */
        label: {
            type: String,
            required: true
        },
        /**
         * Description for the argument
         */
        description: {
            type: String,
            required: false,
            default: null
        },
        /**
         * Initial value from parent
         */
        initial_value: {
            required: false
        },
        /**
         * Event bus to communicate with parent, e.g. for value changes
         */
        parent_event_bus: {
            type: Vue,
            required: true
        },
        /**
         * Event for value changes
         */
        value_change_event: {
            type: String,
            required: true
        },
        /**
         * Active status, if false the input is disabled
         */
        enabled: {
            type: Boolean,
            required: true
        },
    },
    data(){
        return {
            /**
             * Current value, e.g. text, selected files and so on.
             */
            current_value: undefined
        }
    },
    beforeMount(){
        this.initialize()
    },
    methods: {
        /**
         * Set current_value to initial_value if initial_value is not empty.
         * And initializes the watcher for current_value, so the first set of current value
         * is not send to the parent
         * 
         * Called in beforeMount()
         */
        initialize(){
            if(this.value_type === String){
                /**
                 * Special behavior for (literal) strings, see:
                 * https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/instanceof#examples
                 */
                this.current_value = typeof(this.initial_value) === 'string' ? this.initial_value : this.default_value
            } else if(this.value_type === Number){
                /**
                 * Special behavior for numbers are needed
                 */
                this.current_value = !Number.isNaN(this.initial_value) ? this.initial_value : this.default_value
            } else if(this.value == undefined) {
                this.current_value = this.initial_value
            } else {
                this.current_value = this.initial_value instanceof this.value_type ? this.initial_value : this.default_value
            }
            
            /**
             * Initialize watcher after selected_file is initially set,
             * so the first change is not reported.
            */
            this.$watch("current_value", this.sendValueChangeEvent)
        },
        /**
         * Emits value_change_event on parent_event_bus with new value
         * 
         * @param {any} new_value
         */
        sendValueChangeEvent(new_value){
            this.parent_event_bus.$emit(this.value_change_event, this.name, new_value)
        } 
    },
    computed: {
        /**
         * Returns the label while with all whitespaces and non word characters replaced by dash.
         * 
         * @returns {string}
         */
        sanitized_label(){
            return this.label.replace(/\W|\s/g, '-')
        },
        /**
         * Returns the default value for current_value.
         * current_value is initialized with the returned value
         * if inital_value does not match the type returned by value_type.
         * 
         * Can be overriden in components.
         * 
         * @return  {null}
         */
        default_value(){
            return undefined
        },
        /**
         * Returns the expected type for current_value and inital_value
         *
         * Can be overriden in components.
         */
        value_type(){
            return undefined
        }
    }
}