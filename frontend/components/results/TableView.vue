<template>
    <div class="table-view">
        <h2>{{ result_file_header }}</h2>
        <div v-if="result_file_download_status == result_file_download_status_map.FINISHED">
            <p v-if="result_file_description">{{ result_file_description }}</p>
            <div class="table-container">
                <table class="table table-sm table-striped">
                    <thead>
                        <tr>
                            <th @click="sort(colname)" v-for="colname in columns" :key="colname">
                                <span>{{ colname }}</span>
                                <i v-if="colname == sort_by" :class="{'fa-caret-down': sort_asc, 'fa-caret-up': !sort_asc}" class="fa ms-2"></i>
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr v-for="(row, row_idx) in displayed_data" :key="`col${row_idx}`">
                            <td v-for="(content, col_idx) in row" :key="`col${col_idx}`">
                                {{content}}
                            </td>
                        </tr>
                        <tr v-if="data.length == 0">
                            <td :colspan="Math.max(columns.length, 1)">
                                No data available
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
            <Pagination
                v-if="data.length > items_per_page"
                :parent_event_bus="local_event_bus"
                :total_items="data.length"
                :page_change_event="page_change_event"
                :items_per_page="items_per_page"
            ></Pagination>
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
import Vue from 'vue'
import ResultRendererMixin from '@/mixins/result_renderer'

const PAGE_CHANGE_EVENT = "PAGE_CHANGED"

export default {
    mixins: [
        ResultRendererMixin
    ],
    data(){
        return {
            columns: [],                             // Table header
            data: [],                               // Table rows
            displayed_data: [],                     // Rows to be displayed
            local_event_bus: new Vue(),
            items_per_page: 50,
            page: 1,
            sort_by: "",                            // Sort column
            sort_asc: true,                         // Sort direction
            is_sorting: false                       // `true` if sorting is in progress
        }
    },
    mounted(){
        this.downloadFileForRender(
            this.path,
            true,
            true
        ).then(response => {
            return response.json().then(data => {
                this.columns = data.columns
                this.data = data.data
                this.update_displayed_data()
            })
        }).catch(error => {
            if (error !== null && error !== undefined)
                console.error(error)
        })
        this.local_event_bus.$on(PAGE_CHANGE_EVENT, new_page => this.page = new_page)
    },
    methods: {
        /**
         * Sort rows by given column. If the given column is already selected the sort is reversed.
         * @param  {String} new_sort_by Column name to be sorted
         */
        sort(new_sort_by){
            if(!this.is_sorting){
                this.is_sorting = true
                // Sort new if sort column changed
                if(this.sort_by != new_sort_by){
                    this.sort_by = new_sort_by
                    var sort_column = this.columns.indexOf(this.sort_by)
                    // Sort array ascending
                    this.data = this.data.sort((elem_x, elem_y) => {
                        if(elem_x[sort_column] < elem_y[sort_column]){
                            return -1
                        } else if(elem_x[sort_column] > elem_y[sort_column]) {
                            return 1
                        } else {
                            return 0
                        }
                    });
                    // Reverse array if sorting is desc
                    if(!this.sort_asc) this.data = this.data.reverse()
                } else {
                    // Just reverse if column is the same
                    this.sort_asc = !this.sort_asc
                    this.data = this.data.reverse()
                }
                this.update_displayed_data()
                this.is_sorting = false
            }
        },
        /**
         * Updates the displayed rows according the selected page.
         */
        update_displayed_data(){
            let start = (this.page - 1) * this.items_per_page
            this.displayed_data = this.data.slice(
                start,
                start + this.items_per_page
            )
        }
    },
    computed: {
        page_change_event(){
            return PAGE_CHANGE_EVENT
        }
    },
    watch: {
        page(){
            this.update_displayed_data()
        }
    }
}
</script>