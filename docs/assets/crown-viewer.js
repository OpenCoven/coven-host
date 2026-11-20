// Render the beveled OpenCoven mark on demand. All assets work from a local file.
(() => {
  const mount = document.querySelector('#crown-mount');
  const figure = mount.querySelector('figure');
  const canvas = figure.querySelector('canvas');
  const poster = figure.querySelector('img');
  const controls = figure.querySelector('.crown-controls');
  const help = figure.querySelector('#crown-help');
  const expand = figure.querySelector('.expand-crown');
  const dialog = document.querySelector('#crown-dialog');
  const clamp = (value, min, max) => Math.min(max, Math.max(min, value));

  const vertexShader = `
    attribute vec3 position, normal;
    uniform vec2 rotation;
    uniform vec2 scale;
    varying vec3 surface, direction;
    vec3 rotate(vec3 p) {
      vec2 c = cos(rotation), s = sin(rotation);
      vec3 yaw = vec3(c.x*p.x + s.x*p.z, p.y, -s.x*p.x + c.x*p.z);
      return vec3(yaw.x, c.y*yaw.y - s.y*yaw.z, s.y*yaw.y + c.y*yaw.z);
    }
    void main() {
      vec3 p = position / 16384.0;
      // A shallow curve gives the crown a convex face and a concave reverse.
      p.z -= 0.28 * p.x*p.x;
      surface = rotate(p);
      direction = rotate(vec3(normal.x + 0.56*p.x*normal.z, normal.y, normal.z));
      float w = 1.0 - surface.z / 5.0;
      gl_Position = vec4(surface.xy * scale, -surface.z / 3.0, w);
    }`;
  const fragmentShader = `
    precision highp float;
    varying vec3 surface, direction;
    float softbox(vec3 ray, vec3 light, float width) {
      return exp(-dot(ray-light, ray-light) / width);
    }
    void main() {
      vec3 n = normalize(direction);
      vec3 v = normalize(vec3(0., 0., 5.) - surface);
      vec3 r = reflect(-v, n);
      vec3 metal = vec3(.51, .31, .72);
      float diffuse = max(dot(n, normalize(vec3(-.5, .8, 1.))), 0.);
      float rim = pow(1. - abs(dot(n, v)), 2.);
      vec3 color = metal * (.21 + .56*diffuse) + rim*vec3(.24,.18,.34);
      color += vec3(.72,.65,.86) * softbox(r, normalize(vec3(-.82,.48,1.)), .12);
      color += vec3(.45,.29,.78) * softbox(r, normalize(vec3(.72,-.28,.8)), .22);
      color += vec3(.72,.73,.84) * softbox(r, normalize(vec3(-.2,1.,-.3)), .18);
      // A tall studio strip picks out the polished bevel as the object turns.
      float band = (r.x + .18) / .055;
      float strip = exp(-band*band) * smoothstep(-.72,.24,r.z);
      color += vec3(.43,.39,.48) * strip;
      gl_FragColor = vec4(pow(min(color, vec3(1.)), vec3(.80)), 1.);
    }`;

  function createRenderer(gl) {
    const program = gl.createProgram();
    for (const [type, source] of [[gl.VERTEX_SHADER, vertexShader], [gl.FRAGMENT_SHADER, fragmentShader]]) {
      const shader = gl.createShader(type);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
      gl.attachShader(program, shader);
      gl.deleteShader(shader);
    }
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    gl.useProgram(program);

    const bytes = Uint8Array.from(atob(crownMesh.data), char => char.charCodeAt(0));
    const offset = crownMesh.vertices * 12;
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, bytes.subarray(0, offset), gl.STATIC_DRAW);
    for (const [name, start] of [['position', 0], ['normal', 6]]) {
      const attr = gl.getAttribLocation(program, name);
      gl.enableVertexAttribArray(attr);
      gl.vertexAttribPointer(attr, 3, gl.SHORT, name === 'normal', 12, start);
    }
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, bytes.subarray(offset), gl.STATIC_DRAW);
    gl.enable(gl.DEPTH_TEST);
    const rotation = gl.getUniformLocation(program, 'rotation');
    const scale = gl.getUniformLocation(program, 'scale');
    const maxSide = Math.min(4096, gl.getParameter(gl.MAX_RENDERBUFFER_SIZE), ...gl.getParameter(gl.MAX_VIEWPORT_DIMS));
    return (yaw, pitch, zoom) => {
      const {width, height} = canvas.getBoundingClientRect();
      if (!width || !height) return;
      const ratio = Math.min(window.devicePixelRatio || 1, maxSide / Math.max(width, height));
      const w = Math.max(1, Math.round(width * ratio)), h = Math.max(1, Math.round(height * ratio));
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      gl.viewport(0, 0, w, h);
      const fit = .78 * zoom * Math.min(1, w / h);
      gl.uniform2f(rotation, yaw, pitch);
      gl.uniform2f(scale, fit * h / w, fit);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.drawElements(gl.TRIANGLES, (bytes.length - offset) / 2, gl.UNSIGNED_SHORT, 0);
    };
  }

  const home = {yaw: -.42, pitch: .12, zoom: 1};
  const view = {...home};
  let render, frame;
  function redraw() {
    if (!frame && render) frame = requestAnimationFrame(() => {
      frame = 0;
      if (render) render(view.yaw, view.pitch, view.zoom);
    });
  }
  function showLiveView(enabled) {
    canvas.hidden = controls.hidden = help.hidden = !enabled;
    expand.hidden = !enabled || typeof dialog.showModal !== 'function';
    poster.hidden = enabled;
  }
  function start() {
    try {
      const gl = canvas.getContext('webgl', {alpha: true, antialias: true});
      if (!gl) return;
      render = createRenderer(gl);
      showLiveView(true);
      redraw();
    } catch (error) {
      console.warn('Crown viewer unavailable.', error);
      showLiveView(false);
    }
  }
  canvas.addEventListener('webglcontextlost', event => {
    event.preventDefault();
    render = null;
    if (dialog.open) dialog.close();
    showLiveView(false);
  });
  canvas.addEventListener('webglcontextrestored', start);

  function changeZoom(value) {
    view.zoom = clamp(value, .6, 2.5);
    controls.querySelector('[data-action="out"]').disabled = view.zoom <= .6;
    controls.querySelector('[data-action="in"]').disabled = view.zoom >= 2.5;
  }
  function turn(dx, dy) {
    view.yaw += dx;
    view.pitch = clamp(view.pitch + dy, -Math.PI/2, Math.PI/2);
  }
  function setExpanded(expanded) {
    const label = expanded ? 'Close expanded viewer' : 'Expand crown viewer';
    expand.title = label;
    expand.setAttribute('aria-label', label);
    expand.querySelector('use').setAttribute('href', expanded ? '#collapse' : '#expand');
  }
  dialog.addEventListener('close', () => {
    mount.append(figure);
    mount.style.minHeight = '';
    setExpanded(false);
    if (!expand.hidden) expand.focus({preventScroll: true});
    redraw();
  });
  function act(action) {
    switch (action) {
      case 'left': turn(-Math.PI/8, 0); break;
      case 'right': turn(Math.PI/8, 0); break;
      case 'up': turn(0, -Math.PI/12); break;
      case 'down': turn(0, Math.PI/12); break;
      case 'in': changeZoom(view.zoom * 1.2); break;
      case 'out': changeZoom(view.zoom / 1.2); break;
      case 'reset': Object.assign(view, home); changeZoom(home.zoom); break;
      case 'expand':
        if (dialog.open) dialog.close();
        else {
          mount.style.minHeight = mount.offsetHeight + 'px';
          dialog.append(figure);
          setExpanded(true);
          dialog.showModal();
          canvas.focus({preventScroll: true});
        }
        break;
    }
    redraw();
  }
  figure.addEventListener('click', event => {
    const button = event.target.closest('button[data-action]');
    if (button) act(button.dataset.action);
  });
  const keys = {ArrowLeft:'left', ArrowRight:'right', ArrowUp:'up', ArrowDown:'down', '+':'in', '=':'in', '-':'out', Home:'reset'};
  canvas.addEventListener('keydown', event => {
    if (event.altKey || event.ctrlKey || event.metaKey || !Object.hasOwn(keys, event.key)) return;
    event.preventDefault();
    act(keys[event.key]);
  });

  canvas.addEventListener('wheel', event => {
    if (!dialog.open || event.ctrlKey || event.metaKey) return;
    event.preventDefault();
    const delta = event.deltaMode === 0 ? event.deltaY : Math.sign(event.deltaY) * 40;
    changeZoom(view.zoom * Math.exp(-clamp(delta, -250, 250) * .002));
    redraw();
  }, {passive: false});

  const pointers = new Map();
  const distance = ([a, b]) => Math.hypot(b.x-a.x, b.y-a.y);
  canvas.addEventListener('pointerdown', event => {
    if (event.button !== 0 || pointers.size >= 2) return;
    canvas.setPointerCapture(event.pointerId);
    pointers.set(event.pointerId, {x:event.clientX, y:event.clientY});
  });
  canvas.addEventListener('pointermove', event => {
    const last = pointers.get(event.pointerId);
    if (!last) return;
    const initial = [...pointers.values()];
    pointers.set(event.pointerId, {x:event.clientX, y:event.clientY});
    if (pointers.size === 2) {
      const previousDistance = distance(initial);
      if (previousDistance > 0) changeZoom(view.zoom * distance([...pointers.values()]) / previousDistance);
    } else {
      const angle = Math.PI / Math.max(100, canvas.clientWidth / 2);
      turn((event.clientX-last.x)*angle, (event.clientY-last.y)*angle);
    }
    redraw();
  });
  canvas.addEventListener('lostpointercapture', event => pointers.delete(event.pointerId));
  new ResizeObserver(redraw).observe(canvas);
  window.addEventListener('resize', redraw);
  start();
})();
