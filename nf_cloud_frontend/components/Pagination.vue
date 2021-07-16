<template>
    <nav v-if="total > 1" class="mt-3">
        <div class="col d-flex justify-content-center">
            <ul class="pagination flex-wrap">
                <li v-for="page in numbering" :key="page" class="page-item" :class="{'active': page == current}">
                    <button v-if="page != null" class="page-link" type="button" @click="goToPage(page)">
                        {{ page }}
                    </button>
                    <button v-else page class="page-link" type="button" disabled>
                        ...
                    </button>
                </li>
                <li class="page-item">
                    <input v-on:keyup.enter="setPageManually($event)" min="1" class="form-control text-center text-decoration-underline" type="number" placeholder="Go to page...">
                </li>
            </ul>
        </div>
    </nav>
</template>


<script>
import Vue from 'vue'

export default {
    props: {
        // Communication bus with parent
        parent_event_bus: {
            type: Vue,
            required: true
        },
        // Event to emit when changing a page
        page_change_event: {
            type: String,
            required: false,
            default: "PAGE_CHANGED"
        },
        total_items: {
            type: Number,
            required: true
        },
        items_per_page: {
            type: Number,
            required: true
        }
    },
    data(){
        return {
            current: 1
        }
    },
    methods: {
        goToPage(page){
            if(page < 1){
                page = 1
            }
            if(page > this.total){
                page = this.total
            }
            this.current = page
            this.parent_event_bus.$emit(this.page_change_event, this.current)
        },
        setPageManually(event){
            var page = parseInt(event.target.value)
            this.goToPage(page)
            event.target.value = null
        }
    },
    computed: {
        total(){
            return Math.ceil(this.total_items / this.items_per_page)
        },
        numbering(){
            var numbering = []
            for(var i = this.current - 2; i <= this.current + 2; i++) numbering.push(i)
            // Remove all elements which are suited between 1 and the total number of pages
            numbering = numbering.filter(page => 0 < page && page <= this.total)
            if(numbering.length > 0){
                // If first element is greater than 1 add "1"  and null a the start of the list (null is later replaced with "...")
                if(numbering[0] > 1) numbering.unshift(1, null)
                // If last element is smaller then the total number of pages add null and the last page to the numbering.
                if(numbering[numbering.length - 1] < this.total) numbering.push(null, this.total)
            }
            return numbering
        }
    },
    watch: {
        total(){
            // If the amount of pages is changes jumpt to page 1
            this.goToPage(1)
        }
    }

}
</script>