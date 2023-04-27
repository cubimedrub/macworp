<template>
    <div>
        <h1>Documentation</h1>
        <ul>
            <li v-for="article of articles" :key="article.slug">
                <NuxtLink :to="{ name: 'docs-slug', params: { slug: article.slug } }">
                    {{ article.title }} - {{ article.description }}
                </NuxtLink>
            </li>
        </ul>
    </div>
</template>

<script>
export default {
    async asyncData({ $content, params }) {
        const articles = await $content('docs')
        .only(['title', 'slug', 'description'])
        .sortBy('createdAt', 'asc')
        .fetch()
        return {
            articles
        }
    }
}
</script>