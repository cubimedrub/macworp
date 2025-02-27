<template>
    <div v-resize:debounce.100="onResize">
        <h2> {{ result_file_header }} </h2>
        <div v-if="result_file_download_status == result_file_download_status_map.FINISHED">
            <div ref="plot-container"></div>
            <div class="row">
                <div class="col col-md-11 d-flex justify-content-center">
                    <p v-if="result_file_description">  {{ result_file_description }} </p>
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
        <div v-if="result_file_download_status == result_file_download_status_map.FETCHING" class="d-flex justify-content-center">
            <Spinner></Spinner>
        </div>
        <div v-if="result_file_download_status == result_file_download_status_map.NOT_FOUND">
            <p>
                {{ result_file_not_found_message }}
            </p>
        </div>
        <div v-if="result_file_download_status == result_file_download_status_map.FILESIZE_TOO_LARGE">
            <p>
                {{ result_file_too_large_message }}
            </p>
        </div>
    </div>
</template>

<script>
import Plotly from 'plotly.js-dist-min'
import ResultRendererMixin from '@/mixins/result_renderer'

/**
 * Configuration for ploty plots
 */
const PLOTLY_CONFIG = {
    responsive: false
}

/**
 * VueJS directive to resize the plot.
 */
const directives = {};
if (typeof window !== "undefined") {
    directives.resize = require("vue-resize-directive");
}


export default {
    mixins: [
        ResultRendererMixin
    ],
    directives,
    data(){
        return {
            plot_data: null,
            plot_layout: null,
            modal_identifier: ""
        }
    },
    initialized(){
        this.modal_identifier = `${this.path}-modal-${this._uid}`
    },
    mounted(){
        this.downloadFileForRender(this.path, true, false).then(response => {
            response.json().then(data => {
                this.plot_data = data.data
                this.plot_layout = data.layout
            }).then(() => {
                Plotly.newPlot(
                    this.$refs['plot-container'],
                    this.plot_data,
                    this.plot_layout,
                    PLOTLY_CONFIG
                )
            })
        }).catch(error => {
            if (error !== null && error !== undefined)
                console.error(error)
        })
    },
    beforeDestroy() {
        //events.forEach(event => this.$el.removeAllListeners(event.completeName));
        if (this.$refs['plot-container'])
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
            if (this.$refs['plot-container'])
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