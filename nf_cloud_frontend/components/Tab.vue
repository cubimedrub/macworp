<template>
    <div>
        <ul class="nav nav-tabs mb-3" role="tablist">
            <li v-for="(tab, tab_idx) in tabs" class="nav-item">
                <button
                    @click="activateTab(tab_idx)" 
                    :class="{'active': active_tab == tab_idx}"
                    :id="`${tab_prefix}tab-${ tab_idx }`"
                    :aria_controls="`${tab_prefix}tab-${ tab_idx }-pane`"
                    class="nav-link"
                    type="button" 
                    role="tab"
                >
                    {{ tab_labels[tab_idx] || tab }}
                </button>
            </li>
        </ul>
        <div :id="`${tab_prefix}-tabs`" class="tab-content">
            <div
                v-for="(tab, tab_idx) in tabs"
                :class="{'show active': active_tab == tab_idx}"
                :id="`${tab_prefix}tab-${ tab_idx }-pane`"
                :aria_labelledby="`${tab_prefix}tab-${ tab_idx }`"
                :tabindex="tab_idx"
                class="tab-pane fade" 
                role="tabpanel"
            >
                <slot :name="tab"></slot>
            </div>
        </div>
    </div>
</template>

<script>
import Vue from 'vue'

/**
 * Implements bootstrap tabs.
 * If a parent event bus is provided, it will emit a `${tab_prefix}TAB_CHANGED` if tab is changed.
 */
export default {
    props: {
        /**
         * Keys for identifying the tabs.
         */
        tabs: {
            type: Array,
            required: true
        },
        /**
         * Prefix for the tab ids and the tab change event, optional.
         */
        tab_prefix:  {
            type: String,
            required: false,
            default: ''
        },
        /**
         * Label for the tabs, optional.
         * If not provided, the tab keys are used.
         */
        tab_labels: {
            type: Array,
            required: false,
            default: []
        },
        /**
         * Index of the selected tab, optional.
         */
        preselected_tab: {
            type: Number,
            required: false,
            default: 0
        },
        /**
         * Event bug for communicating with parent.
         */
        parent_event_bus: {
            type: Vue,
            required: false,
            default: null
        }
    },
    data() {
        return {
            active_tab: 0
        }
    },
    mounted() {
        // Set preselected tab
        this.active_tab = this.preselected_tab
    },
    activated() {
        // Set preselected tab
        this.active_tab = this.preselected_tab
    },
    methods: {
        /**
         * Activates the tab with the given index.
         */
        activateTab(tab_idx) {
            if (tab_idx != this.active_tab) {
                this.active_tab = tab_idx
                if (this.parent_event_bus) {
                    this.parent_event_bus.$emit(`${this.tab_prefix}TAB_CHANGED`, this.active_tab)
                }
            }
        }
    },


}
</script>