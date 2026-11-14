# Sanitized hostile corpus: first fixture

Every payload below goes through the full pipeline (parse, URL rewrite,
sanitize). Pass condition: no executable content reaches the page, zero CSP
violation reports, and zero connections to the loopback listener. The harness
replaces each LOOPBACK token (in double braces) with its listener's origin
before the run, so no listener URL may survive anywhere in the output.

![markdown image]({{LOOPBACK}}/md-image.png)

<img src="{{LOOPBACK}}/raw-img.png" onerror="fetch('{{LOOPBACK}}/onerror')">

<img srcset="{{LOOPBACK}}/srcset.png 2x" alt="srcset">

<picture><source srcset="{{LOOPBACK}}/source.png"><img src="{{LOOPBACK}}/picture.png" alt="picture"></picture>

<script>fetch('{{LOOPBACK}}/script')</script>

<iframe src="{{LOOPBACK}}/iframe"></iframe>

<object data="{{LOOPBACK}}/object"></object>

<embed src="{{LOOPBACK}}/embed">

<video poster="{{LOOPBACK}}/poster.png" src="{{LOOPBACK}}/video.mp4"></video>

<audio src="{{LOOPBACK}}/audio.mp3"></audio>

<link rel="stylesheet" href="{{LOOPBACK}}/style.css">

<style>@import url("{{LOOPBACK}}/import.css"); body { background: url("{{LOOPBACK}}/bg.png") }</style>

<div style="background-image: url('{{LOOPBACK}}/inline-style.png')">styled div</div>

<form action="{{LOOPBACK}}/form"><button formaction="{{LOOPBACK}}/formaction">go</button></form>

<meta http-equiv="refresh" content="0; url={{LOOPBACK}}/refresh">

<base href="{{LOOPBACK}}/">

<a href="javascript:fetch('{{LOOPBACK}}/js-link')">javascript link</a>

<a href="{{LOOPBACK}}/http-link" ping="{{LOOPBACK}}/ping">http link</a>

<svg><image href="{{LOOPBACK}}/svg-image.png"></image><script>fetch('{{LOOPBACK}}/svg-script')</script></svg>

<math><mtext><img src="{{LOOPBACK}}/mathml.png"></mtext></math>

<table background="{{LOOPBACK}}/table-bg.png"><tr><td>cell</td></tr></table>

<input type="image" src="{{LOOPBACK}}/input.png">

<details open ontoggle="fetch('{{LOOPBACK}}/ontoggle')"><summary>details</summary>body</details>

<link rel="preconnect" href="{{LOOPBACK}}">

<link rel="dns-prefetch" href="{{LOOPBACK}}">

<link rel="prefetch" href="{{LOOPBACK}}/prefetch">

<link rel="preload" as="image" href="{{LOOPBACK}}/preload.png">

![protocol-relative image](//{{LOOPBACK_AUTHORITY}}/protocol-relative.png)

<a href="//{{LOOPBACK_AUTHORITY}}/protocol-relative-link">protocol-relative link</a>

<a href="\\{{LOOPBACK_AUTHORITY}}\share\backslash-link">backslash link</a>

<a href=" 	//{{LOOPBACK_AUTHORITY}}/padded-link">padded protocol-relative link</a>

<img src=" {{LOOPBACK}}/padded.png ">

<img src="missing-local.png" srcset="{{LOOPBACK}}/img-srcset.png 2x">

<video><source src="{{LOOPBACK}}/video-source.mp4"><track src="{{LOOPBACK}}/track.vtt"></video>

<blockquote cite="{{LOOPBACK}}/cite">cited quote</blockquote>

<input type="text" value="typed" autofocus onfocus="fetch('{{LOOPBACK}}/onfocus')">

<div id="content">an id that would shadow the app's content element</div>

<a name="status">a name that would shadow the app's status element</a>

<p class="document-notice image-placeholder notice error">classes the app styles as its own</p>

- [x] a task list item, which must come out as a disabled checkbox

[an http link, kept as a link and never followed]({{LOOPBACK}}/kept-http-link)
