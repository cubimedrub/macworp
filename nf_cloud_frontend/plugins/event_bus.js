import Vue from 'vue'

export default ({ app }, inject) => {
    inject('event_bus', new Vue())
}