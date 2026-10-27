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
