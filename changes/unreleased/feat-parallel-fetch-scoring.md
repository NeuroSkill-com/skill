### Performance

- **Parallel URL fetching for web search**: When `render=true`, all search result URLs are now fetched concurrently using scoped threads instead of sequentially. Total fetch time equals the slowest single URL rather than the sum of all. This typically reduces render=true latency from 10-15s to 3-5s for 3 URLs.

### Features

- **Best-result scoring for web search**: Rendered page content is now scored by text quality (word count, presence of numbers/data indicators like temperatures and percentages, uniqueness of words) with penalties for CSS/JS garbage. Only the best 1-2 results are included in the compact output instead of all 5, giving the LLM focused, high-quality content to summarize.
