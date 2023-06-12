<template>
    <div v-resize:debounce.100="onResize">
        <h2> {{ header }} </h2>
        <div ref="plot-container"></div>
        <div class="row">
            <div class="col col-md-11 d-flex justify-content-center">
                <p>  {{ description }} </p>
            </div>
            <div class="col col-md-1 d-flex justify-content-center">
                <button @click="show" type="button" class="btn btn-primary btn-sm">
                    <i class="fas fa-cog"></i>
                </button>
            </div>
        </div>
        
        <div :ref="modal_identifier" class="modal" tabindex="-1">
            <div class="modal-dialog">
                <div class="modal-content">
                    <div class="modal-header">
                        <h5 class="modal-title">
                            Settings
                        </h5>
                        <button type="button" class="btn-close" data-bs-dismiss="modal" aria-label="Close"></button>
                    </div>
                    <div class="modal-body">
                        <h6>Data</h6>
                        <client-only>
                            <JsonEditorVue v-model="plot_data" />
                        </client-only>
                        <h6>Layout</h6>
                        <client-only>
                            <JsonEditorVue v-model="plot_layout" />
                        </client-only>
                    </div>
                    <div class="modal-footer">
                        <button @click="dismiss" type="button" class="btn btn-secondary" data-bs-dismiss="modal">
                            Close
                        </button>
                    </div>
                </div>
            </div>
        </div>
        
    </div>
</template>

<script>
import ResultMixin from '../mixins/result'
import Plotly from 'plotly.js-dist-min'

const PLOTLY_CONFIG = {
    responsive: false
}

const directives = {};
if (typeof window !== "undefined") {
    directives.resize = require("vue-resize-directive");
}


export default {
    mixins: [
        ResultMixin
    ],
    directives,
    props: {
        header: {
            type: String,
            required: true
        },
        path: {
            type: String,
            required: true
        },
        description: {
            type: String,
            required: true
        }
    },
    data(){
        return {
            plot_data: null,
            plot_layout: null,
            modal_identifier: ""
        }
    },
    initialized(){
        this.modal_identifier = `${this.header}-modal-${this._uid}`
    },
    mounted(){
        this.authenticateFileDownload(this.path).then(url => {
            return fetch(`${url}&is-inline=1`).then(response => {
                return response.json()
            }).then(data => {
                this.plot_data = data.data
                this.plot_layout = data.layout
                return Promise.resolve()
            })
        }).then(() => {
            Plotly.newPlot(
                this.$refs['plot-container'],
                this.plot_data,
                this.plot_layout,
                PLOTLY_CONFIG
            )
        })
    },
    beforeDestroy() {
        //events.forEach(event => this.$el.removeAllListeners(event.completeName));
        Plotly.purge(this.$refs['plot-container']);
    },
    methods: {
        /**
         * Opens the modal.
         */
        show(){
            this.openModal(this.modal_identifier)
        },
        /**
         * Closes modal without any action.
         */
        dismiss(){
            this.closeModal(this.modal_identifier)
        },
        /**
         * Updates the plot once the data was changed
         */
        updatePlot(){
            Plotly.newPlot(
                this.$refs['plot-container'],
                this.plot_data,
                this.plot_layout,
                PLOTLY_CONFIG
            )
        },
        /**
         * Resize the plot.
         */
        onResize() {
            Plotly.Plots.resize(this.$refs['plot-container']);
        }
    },
    watch: {
        plot_data(){
            this.updatePlot()
        },
        plot_layout(){
            this.updatePlot()
        }
    }
}
</script>