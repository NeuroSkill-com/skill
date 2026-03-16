### Bugfixes

- **Use realistic browser User-Agent for DuckDuckGo and web_fetch**: replaced bot-like User-Agent strings (`SkillBot/1.0`, `NeuroSkill-LLM-Tool/1.0`) with a standard Chrome browser UA to avoid captcha and bot-detection blocks. Also added `Accept`, `Accept-Language`, and `Referer` headers to DuckDuckGo requests.
