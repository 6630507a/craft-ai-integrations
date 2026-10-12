"""Read-only-install SAM 3 API probes. Uses a public-domain NASA sample.
Writes only test uploads, ComfyUI outputs, and this probe's evidence folder.
Never uses PhotoCraft's interactive node ID or modifies installed nodes/models.
"""
import base64
import hashlib
import io
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from PIL import Image, ImageChops

BASE = 'http://127.0.0.1:8000'
ROOT = Path.home() / 'AppData/Local/hermes/cache/scratch/sam3-headless-evidence'
ROOT.mkdir(exist_ok=True)
SOURCE = Path.home() / 'AppData/Local/hermes/cache/scratch/astronaut.png'
RESULTS = []
RUN = str(time.time_ns())
NODE = 'sam3_research_' + RUN


def request(path, payload=None, timeout=180):
    headers = {}
    data = None
    if payload is not None:
        data = json.dumps(payload).encode()
        headers['Content-Type'] = 'application/json'
    req = urllib.request.Request(BASE + path, data=data, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.read()
    except urllib.error.HTTPError as e:
        raise RuntimeError(f'{path}: HTTP {e.code}: {e.read().decode()[:1500]}') from e


def get_json(path, payload=None):
    return json.loads(request(path, payload))


def upload():
    boundary = 'sam3research' + RUN
    data = SOURCE.read_bytes()
    name = 'sam3-research-astronaut-' + RUN + '.png'
    body = (f'--{boundary}\r\nContent-Disposition: form-data; name="type"\r\n\r\ninput\r\n'
            f'--{boundary}\r\nContent-Disposition: form-data; name="image"; filename="{name}"\r\n'
            'Content-Type: image/png\r\n\r\n').encode() + data + f'\r\n--{boundary}--\r\n'.encode()
    req = urllib.request.Request(BASE + '/upload/image', data=body,
                                 headers={'Content-Type': 'multipart/form-data; boundary=' + boundary})
    with urllib.request.urlopen(req, timeout=30) as r:
        result = json.load(r)
    assert result.get('type') == 'input'
    return result['name']


def node(kind, **inputs):
    return {'class_type': kind, 'inputs': inputs}


def core(filename):
    return {'1': node('LoadImage', image=filename),
            '2': node('LoadSAM3Model', precision='auto', compile=False)}


def output(graph, source='3', boxes=None, scores=None):
    graph['90'] = node('MaskToImage', mask=[source, 0])
    graph['91'] = node('SaveImage', images=['90', 0], filename_prefix='sam3-research/' + RUN)
    if boxes is not None:
        graph['92'] = node('PreviewAny', source=[source, boxes])
    if scores is not None:
        graph['93'] = node('PreviewAny', source=[source, scores])
    return graph


def metrics(data):
    im = Image.open(io.BytesIO(data)).convert('L')
    assert im.size == (512, 512), im.size
    binary = im.point(lambda x: 255 if x >= 128 else 0)
    hist = binary.histogram()
    return {'dimensions': list(im.size), 'bbox_xyxy': binary.getbbox(),
            'foreground_pixels': hist[255], 'sha256': hashlib.sha256(data).hexdigest()}


def run(label, graph):
    start = time.perf_counter()
    submission = get_json('/prompt', {'prompt': graph, 'client_id': 'sam3-research-' + RUN})
    pid = submission['prompt_id']
    (ROOT / (label + '-graph.json')).write_text(json.dumps(graph, indent=2))
    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        rec = get_json('/history/' + pid).get(pid)
        if rec and rec.get('status', {}).get('status_str') == 'error':
            raise RuntimeError(json.dumps(rec['status'])[:2500])
        if rec and rec.get('status', {}).get('completed'):
            break
        time.sleep(0.05)
    else:
        raise TimeoutError(label)
    elapsed = time.perf_counter() - start
    saved = rec.get('outputs', {}).get('91', {}).get('images', [])
    masks = []
    for i, meta in enumerate(saved):
        assert meta.get('type') == 'output'
        data = request('/view?' + urllib.parse.urlencode(meta))
        path = ROOT / f'{label}-{i}.png'
        path.write_bytes(data)
        masks.append({**metrics(data), 'path': str(path)})
    item = {'case': label, 'prompt_id': pid, 'wall_seconds': round(elapsed, 4),
            'mask_count': len(masks), 'masks': masks}
    for key, nid in [('boxes', '92'), ('scores', '93')]:
        values = rec.get('outputs', {}).get(nid, {}).get('text', [])
        if values:
            item[key] = json.loads(values[0])
    (ROOT / (label + '-history.json')).write_text(json.dumps(rec, indent=2))
    RESULTS.append(item)
    (ROOT / 'results.json').write_text(json.dumps(RESULTS, indent=2))
    print(json.dumps(item), flush=True)
    return item


def points_graph(filename, refinement=0, all_candidates=True, points=None, negative=None, box=None):
    g = core(filename)
    points = points or [(220, 110)]
    g['10'] = node('SAM3PointCollector', image=['1', 0], points_store='{}',
                   coordinates=json.dumps([{'x': x, 'y': y} for x, y in points]),
                   neg_coordinates=json.dumps([{'x': x, 'y': y} for x, y in negative or []]))
    inputs = dict(sam3_model_config=['2', 0], image=['1', 0],
                  positive_points=['10', 0], negative_points=['10', 1],
                  refinement_iterations=refinement, use_multimask=True,
                  output_best_mask=not all_candidates)
    if box:
        g['11'] = node('SAM3BBoxCollector', image=['1', 0], bboxes=json.dumps([box]), neg_bboxes='[]')
        inputs['box'] = ['11', 0]
    g['3'] = node('SAM3Segmentation', **inputs)
    return output(g, boxes=3, scores=4)


def interactive(filename, prompts):
    g = core(filename)
    g[NODE] = node('SAM3InteractiveCollector', sam3_model_config=['2', 0], image=['1', 0],
                   multi_prompts_store=json.dumps(prompts))
    return output(g, source=NODE)


def main():
    stats = get_json('/system_stats')
    assert stats['system']['comfyui_version']
    queue = get_json('/queue')
    if queue['queue_running'] or queue['queue_pending']:
        raise RuntimeError('Refusing to run probes while the server is busy')
    (ROOT / 'system-stats.json').write_text(json.dumps(stats, indent=2))
    schema = get_json('/object_info')
    sam = {k: v for k, v in schema.items() if 'sam3' in k.lower()}
    assert len(sam) == 19
    (ROOT / 'node-schemas.json').write_text(json.dumps(sam, indent=2))
    filename = upload()
    run('candidates-face', points_graph(filename))
    run('candidates-face-refine1', points_graph(filename, refinement=1))
    run('candidates-face-refine3', points_graph(filename, refinement=3))
    run('points-face-and-suit', points_graph(filename, all_candidates=False,
                                            points=[(220, 110), (256, 400)]))
    run('points-negative', points_graph(filename, all_candidates=False,
                                       points=[(256, 400)], negative=[(220, 110)]))
    g = core(filename)
    g['3'] = node('SAM3Grounding', sam3_model_config=['2', 0], image=['1', 0],
                  confidence_threshold=0.2, text_prompt='eye', max_detections=-1)
    run('ground-eyes-all', output(g, boxes=2, scores=3))
    g = core(filename)
    g['3'] = node('SAM3Grounding', sam3_model_config=['2', 0], image=['1', 0],
                  confidence_threshold=0.2, text_prompt='eye', max_detections=1)
    run('ground-eyes-top1', output(g, boxes=2, scores=3))
    g = core(filename)
    g['10'] = node('SAM3CreateBox', center_x=0.41, center_y=0.20, width=0.05,
                   height=0.08, is_positive=True)
    g['11'] = node('SAM3CombineBoxes', box_1=['10', 0])
    g['3'] = node('SAM3Grounding', sam3_model_config=['2', 0], image=['1', 0],
                  confidence_threshold=0.2, text_prompt='eye', max_detections=-1,
                  positive_boxes=['11', 0])
    run('ground-eye-positive-box', output(g, boxes=2, scores=3))
    regions = [{'positive_points': [{'x': 203, 'y': 101}], 'name': 'left-eye'},
               {'positive_points': [{'x': 250, 'y': 105}], 'name': 'right-eye'}]
    g = core(filename)
    g['10'] = node('SAM3MultiRegionCollector', image=['1', 0], multi_prompts_store=json.dumps(regions))
    g['3'] = node('SAM3MultipromptSegmentation', sam3_model_config=['2', 0], image=['1', 0],
                  multi_prompts=['10', 0], refinement_iterations=1, use_multimask=True)
    run('multiprompt-two-regions', output(g))
    run('interactive-prepare', interactive(filename, [{'session': RUN}]))
    raw = {'positive_points': [{'x': 220, 'y': 110}]}
    for i in range(3):
        start = time.perf_counter()
        reply = get_json('/sam3/interactive_segment_one', {
            'node_id': NODE, 'prompt_index': 0, 'prompt_name': 'research', 'prompt': raw})
        assert reply.get('num_masks') == 1, reply
        jpeg = base64.b64decode(reply['overlay'])
        assert Image.open(io.BytesIO(jpeg)).size == (512, 512)
        (ROOT / f'hover-{i}.jpg').write_bytes(jpeg)
        item = {'case': 'hover-' + str(i), 'wall_seconds': round(time.perf_counter() - start, 4),
                'num_masks': reply['num_masks'], 'response_keys': list(reply)}
        RESULTS.append(item)
        print(json.dumps(item), flush=True)
    run('interactive-face', interactive(filename, [{**raw, 'session': RUN}]))
    run('interactive-face-negative-box', interactive(filename, [{**raw, 'session': RUN,
        'negative_boxes': [{'x1': 170, 'y1': 60, 'x2': 285, 'y2': 180}]}]))
    a = Image.open(ROOT / 'interactive-face-0.png')
    b = Image.open(ROOT / 'interactive-face-negative-box-0.png')
    equal = ImageChops.difference(a, b).getbbox() is None
    summary = {'negative_box_produced_identical_mask': equal,
               'node_count': len(sam), 'total_cases': len(RESULTS),
               'queue_after': get_json('/queue'), 'research_node_id': NODE}
    (ROOT / 'summary.json').write_text(json.dumps(summary, indent=2))
    (ROOT / 'results.json').write_text(json.dumps(RESULTS, indent=2))
    print('SUMMARY ' + json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
