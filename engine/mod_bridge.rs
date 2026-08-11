//! Live Flash mod bridge — JSON RPC for AVM2 get/set/call/find without reload.
//!
//! Exposed via `Player::mod_bridge_rpc` and desktop HTTP :8768 / web WASM.

use crate::avm2::property::Property;
use crate::avm2::{
    Activation, Error as Avm2Error, FunctionArgs, Object as Avm2Object, TObject, Value as Avm2Value,
};
use crate::context::UpdateContext;
use crate::display_object::{DisplayObject, TDisplayObject, TDisplayObjectContainer};
use crate::string::AvmString;
use serde::Deserialize;
use serde_json::{Value as Json, json};
use std::collections::VecDeque;

const BRIDGE_VERSION: &str = "0.1.0";

#[derive(Debug, Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    value: Option<Json>,
    #[serde(default)]
    args: Option<Vec<Json>>,
    #[serde(default)]
    keywords: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    base64: Option<String>,
    #[serde(default)]
    max_depth: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    prop: Option<String>,
}

pub fn dispatch(context: &mut UpdateContext<'_>, request_json: &str) -> String {
    let req: Request = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "error": format!("invalid json: {e}")}).to_string(),
    };

    let result = match req.op.as_str() {
        "ping" => Ok(op_ping(context)),
        "list_display" => Ok(op_list_display(
            context,
            req.max_depth.unwrap_or(3),
            req.limit.unwrap_or(120),
        )),
        "list_props" => {
            op_list_props(context, req.path.as_deref().unwrap_or("root"), req.limit.unwrap_or(80))
        }
        "get" => op_get(context, req.path.as_deref().unwrap_or("")),
        "set" => op_set(
            context,
            req.path.as_deref().unwrap_or(""),
            req.value.unwrap_or(Json::Null),
        ),
        "call" => op_call(
            context,
            req.path.as_deref().unwrap_or("root"),
            req.method.as_deref().unwrap_or(""),
            req.args.unwrap_or_default(),
        ),
        "find" => Ok(op_find(
            context,
            req.keywords
                .as_deref()
                .unwrap_or("money,tip,score,coin,gold,hp,day,rank,allmoney"),
            req.max_depth.unwrap_or(5),
            req.limit.unwrap_or(60),
        )),
        "list_so" => Ok(op_list_so(context)),
        "get_so" => op_get_so(context, req.name.as_deref().unwrap_or(""), req.limit.unwrap_or(80)),
        "set_so_prop" => op_set_so_prop(
            context,
            req.name.as_deref().unwrap_or(""),
            req.prop.as_deref().unwrap_or(""),
            req.value.unwrap_or(Json::Null),
        ),
        "storage_get" => Ok(op_storage_get(context, req.key.as_deref().unwrap_or(""))),
        "storage_put" => Ok(op_storage_put(
            context,
            req.key.as_deref().unwrap_or(""),
            req.base64.as_deref().unwrap_or(""),
        )),
        // Bind live SharedObject instance onto a field (e.g. ThanksTanks.sharedOb)
        // so game methods like setLevelsUnlocked / loadState stop throwing.
        "bind_so" => op_bind_so(
            context,
            req.path.as_deref().unwrap_or("root/thanksTanks.sharedOb"),
            req.name.as_deref().unwrap_or(""),
        ),
        // Set a property to another live object: path=target.prop path, name=source object path
        "set_ref" => op_set_ref(
            context,
            req.path.as_deref().unwrap_or(""),
            req.name.as_deref().unwrap_or(""),
        ),
        // Set a slot by local name including private fields (scans class traits).
        // path = object path (e.g. root/thanksTanks), prop = field name (e.g. level),
        // name = optional source object path for object values, or use value for primitives.
        "set_slot" => op_set_slot(
            context,
            req.path.as_deref().unwrap_or(""),
            req.prop.as_deref().unwrap_or(""),
            req.name.as_deref(),
            req.value.clone(),
        ),
        // List all class traits (public+private slots/methods) on path — for reverse engineering.
        "list_slots" => op_list_slots(
            context,
            req.path.as_deref().unwrap_or("root"),
            req.limit.unwrap_or(200),
        ),
        other => Ok(json!({"ok": false, "error": format!("unknown op: {other}")})),
    };

    match result {
        Ok(v) => v.to_string(),
        Err(e) => json!({"ok": false, "error": e}).to_string(),
    }
}

fn op_ping(context: &mut UpdateContext<'_>) -> Json {
    let swf = context.root_swf.clone();
    let stage = context.stage;
    let root = stage.root_clip();
    let (sw, sh) = stage.stage_size();
    let so_names: Vec<String> = context.avm2_shared_objects.keys().cloned().collect();
    json!({
        "ok": true,
        "bridge_version": BRIDGE_VERSION,
        "movie_url": swf.url(),
        "swf_version": swf.version(),
        "is_action_script_3": swf.is_action_script_3(),
        "frame_rate": f64::from(swf.frame_rate()),
        "num_frames": swf.num_frames(),
        "stage_width": sw,
        "stage_height": sh,
        "has_root": root.is_some(),
        "shared_object_count": so_names.len(),
        "shared_object_names": so_names,
        "desktop_ready": true,
    })
}

fn dobj_kind(dobj: DisplayObject<'_>) -> &'static str {
    if dobj.as_movie_clip().is_some() {
        "MovieClip"
    } else if dobj.as_edit_text().is_some() {
        "EditText"
    } else if dobj.as_bitmap().is_some() {
        "Bitmap"
    } else if dobj.as_avm1_button().is_some() || dobj.as_avm2_button().is_some() {
        "Button"
    } else {
        "DisplayObject"
    }
}

fn dobj_name(dobj: DisplayObject<'_>) -> String {
    dobj.name()
        .map(|n| n.to_string())
        .unwrap_or_else(|| format!("@{:p}", dobj.as_ptr()))
}

fn class_name_of(obj: Avm2Object<'_>) -> String {
    obj.instance_class().name().local_name().to_string()
}

fn op_list_display(context: &mut UpdateContext<'_>, max_depth: usize, limit: usize) -> Json {
    let mut out: Vec<Json> = Vec::new();
    let mut queue: VecDeque<(DisplayObject<'_>, usize, String)> = VecDeque::new();

    if let Some(root) = context.stage.root_clip() {
        queue.push_back((root, 0, "root".into()));
    } else {
        queue.push_back((DisplayObject::Stage(context.stage), 0, "stage".into()));
    }

    while let Some((dobj, depth, path)) = queue.pop_front() {
        if out.len() >= limit {
            break;
        }
        let child_count = dobj.as_container().map(|c| c.num_children()).unwrap_or(0);
        let class = dobj.object2().map(|o| class_name_of(o.into()));
        out.push(json!({
            "name": dobj_name(dobj),
            "kind": dobj_kind(dobj),
            "class": class,
            "depth": depth,
            "path": path,
            "children": child_count,
            "has_avm2": dobj.object2().is_some(),
        }));
        if depth < max_depth {
            if let Some(container) = dobj.as_container() {
                for child in container.iter_render_list() {
                    if out.len() + queue.len() >= limit * 2 {
                        break;
                    }
                    let cn = dobj_name(child);
                    queue.push_back((child, depth + 1, format!("{path}/{cn}")));
                }
            }
        }
    }

    json!({"ok": true, "nodes": out, "count": out.len()})
}

fn resolve_so<'gc>(context: &mut UpdateContext<'gc>, name: &str) -> Result<Avm2Object<'gc>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("SharedObject name empty".into());
    }
    // Exact match first
    if let Some(so) = context.avm2_shared_objects.get(name).copied() {
        return Ok(so.data());
    }
    // Prefer longest key that ends with /name or equals suffix (avoid "chat" matching everything)
    let mut best: Option<(usize, Avm2Object<'gc>)> = None;
    for (k, so) in context.avm2_shared_objects.iter() {
        if k == name || k.ends_with(name) || k.ends_with(&format!("/{name}")) {
            let score = k.len();
            if best.map(|(s, _)| score >= s).unwrap_or(true) {
                best = Some((score, so.data()));
            }
        }
    }
    if let Some((_, obj)) = best {
        return Ok(obj);
    }
    Err(format!(
        "SharedObject not loaded: {name:?} (have: {:?})",
        context.avm2_shared_objects.keys().collect::<Vec<_>>()
    ))
}

fn resolve_display<'gc>(
    context: &mut UpdateContext<'gc>,
    path: &str,
) -> Result<DisplayObject<'gc>, String> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err("empty display path".into());
    }

    let mut cur: DisplayObject<'gc> = match parts[0] {
        "stage" => DisplayObject::Stage(context.stage),
        "root" => context
            .stage
            .root_clip()
            .ok_or_else(|| "no root clip".to_string())?,
        other => {
            let root = context
                .stage
                .root_clip()
                .ok_or_else(|| "no root clip".to_string())?;
            let c = root
                .as_container()
                .ok_or_else(|| "root is not a container".to_string())?;
            let w = AvmString::new_utf8(context.gc(), other);
            c.child_by_name(w.as_wstr(), false)
                .ok_or_else(|| format!("no child '{other}' under root"))?
        }
    };

    for part in &parts[1..] {
        let container = cur
            .as_container()
            .ok_or_else(|| format!("'{}' is not a container", dobj_name(cur)))?;
        let w = AvmString::new_utf8(context.gc(), *part);
        cur = container
            .child_by_name(w.as_wstr(), false)
            .ok_or_else(|| format!("no child '{part}' under {}", dobj_name(cur)))?;
    }
    Ok(cur)
}

/// Returns (base object, remaining property path with dots).
fn resolve_base_object<'gc>(
    context: &mut UpdateContext<'gc>,
    path: &str,
) -> Result<(Avm2Object<'gc>, String), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("path required".into());
    }

    // SharedObject names often contain dots and slashes (e.g. host//path/Game).
    // Props MUST use '|': so:full/so/name|prop.nested — never split on '.' here.
    if let Some(rest) = path.strip_prefix("so:") {
        if let Some((name, props)) = rest.split_once('|') {
            return Ok((resolve_so(context, name.trim())?, props.to_string()));
        }
        return Ok((resolve_so(context, rest.trim())?, String::new()));
    }

    let (base_path, prop_path) = if let Some(dot) = path.find('.') {
        (&path[..dot], path[dot + 1..].to_string())
    } else {
        (path, String::new())
    };

    let dobj = resolve_display(context, base_path)?;
    let obj = dobj
        .object2()
        .map(|o| o.into())
        .ok_or_else(|| format!("no AVM2 object at display path '{base_path}'"))?;
    Ok((obj, prop_path))
}

/// Resolve one property: public first, then any resolved vtable slot by local name
/// (covers private/internal fields the public lookup misses).
fn get_prop_any<'gc>(
    activation: &mut Activation<'_, 'gc>,
    value: Avm2Value<'gc>,
    part: &str,
) -> Result<Avm2Value<'gc>, String> {
    let name = AvmString::new_utf8(activation.gc(), part);
    match value.get_public_property(name, activation) {
        Ok(v) => return Ok(v),
        Err(pub_err) => {
            let Some(obj) = value.as_object() else {
                return Err(format!("get '{part}': {}", avm2_err(&pub_err)));
            };
            // Prefer slot/const match by local name (private namespaces included).
            for (local_name, _ns, prop) in obj.vtable().resolved_traits().iter() {
                if local_name != name {
                    continue;
                }
                match *prop {
                    Property::Slot { slot_id } | Property::ConstSlot { slot_id } => {
                        return Ok(obj.get_slot(slot_id));
                    }
                    Property::Virtual { get: Some(disp_id), .. } => {
                        // Avoid calling arbitrary getters — they can crash game code.
                        // Fall through unless public already failed for a different reason.
                        let _ = disp_id;
                    }
                    Property::Method { .. } | Property::Virtual { get: None, .. } => {}
                }
            }
            // Numeric array index
            if let Ok(idx) = part.parse::<i32>() {
                if let Some(arr) = obj.as_array_storage() {
                    if let Some(v) = arr.get(idx as usize) {
                        return Ok(v);
                    }
                }
            }
            Err(format!("get '{part}': {}", avm2_err(&pub_err)))
        }
    }
}

fn walk_props<'gc>(
    activation: &mut Activation<'_, 'gc>,
    obj: Avm2Object<'gc>,
    prop_path: &str,
) -> Result<Avm2Value<'gc>, String> {
    if prop_path.is_empty() {
        return Ok(obj.into());
    }
    let parts: Vec<&str> = prop_path.split('.').filter(|p| !p.is_empty()).collect();
    let mut value = Avm2Value::from(obj);
    for part in parts {
        value = get_prop_any(activation, value, part)?;
    }
    Ok(value)
}

fn value_to_json_shallow(value: Avm2Value<'_>) -> Json {
    match value.normalize() {
        Avm2Value::Undefined | Avm2Value::Null => Json::Null,
        Avm2Value::Bool(b) => json!(b),
        Avm2Value::Number(n) => json!(n),
        Avm2Value::Integer(i) => json!(i),
        Avm2Value::String(s) => json!(s.to_string()),
        other => {
            if let Some(obj) = other.as_object() {
                json!({ "__object": class_name_of(obj) })
            } else {
                json!(format!("{other:?}"))
            }
        }
    }
}

/// Slot/const values only — **never call getters** (they can crash game code).
fn list_slot_props(obj: Avm2Object<'_>, limit: usize) -> serde_json::Map<String, Json> {
    let mut props = serde_json::Map::new();
    let vtable = obj.vtable();
    for (name, prop) in vtable.public_properties() {
        if props.len() >= limit {
            break;
        }
        if let Property::Slot { slot_id } | Property::ConstSlot { slot_id } = prop {
            let value = obj.base().get_slot(slot_id);
            props.insert(name.to_string(), value_to_json_shallow(value));
        } else if let Property::Virtual { get: Some(_), .. } = prop {
            // Name only — do not invoke getter
            props
                .entry(name.to_string())
                .or_insert_with(|| json!("[getter]"));
        } else if let Property::Method { .. } = prop {
            props
                .entry(name.to_string())
                .or_insert_with(|| json!("[method]"));
        }
    }
    props
}

fn json_to_value<'gc>(
    activation: &mut Activation<'_, 'gc>,
    v: &Json,
) -> Result<Avm2Value<'gc>, String> {
    match v {
        Json::Null => Ok(Avm2Value::Null),
        Json::Bool(b) => Ok(Avm2Value::Bool(*b)),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    return Ok(Avm2Value::Integer(i as i32));
                }
            }
            Ok(Avm2Value::Number(n.as_f64().unwrap_or(0.0)))
        }
        Json::String(s) => Ok(Avm2Value::String(AvmString::new_utf8(activation.gc(), s))),
        Json::Array(_) | Json::Object(_) => {
            Err("object/array values not supported — use primitives".into())
        }
    }
}

fn avm2_err(e: &Avm2Error<'_>) -> String {
    format!("{e:?}")
}

fn op_list_props(context: &mut UpdateContext<'_>, path: &str, limit: usize) -> Result<Json, String> {
    let (obj, prop_path) = resolve_base_object(context, path)?;
    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let value = walk_props(&mut activation, obj, &prop_path)?;
    let obj = value
        .as_object()
        .ok_or_else(|| "path did not resolve to object".to_string())?;

    // Slots only — no enumerant walk (some SO/data objects blow up on hasNext).
    let props = list_slot_props(obj, limit);
    let _ = &mut activation; // activation kept for future safe gets

    Ok(json!({
        "ok": true,
        "path": path,
        "class": class_name_of(obj),
        "props": props,
        "count": props.len(),
        "note": "slots only; getters/enumerants not invoked (safe)"
    }))
}

fn op_get(context: &mut UpdateContext<'_>, path: &str) -> Result<Json, String> {
    if path.is_empty() {
        return Err("path required".into());
    }
    let (obj, prop_path) = resolve_base_object(context, path)?;
    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let value = walk_props(&mut activation, obj, &prop_path)?;
    Ok(json!({
        "ok": true,
        "path": path,
        "value": value_to_json_shallow(value)
    }))
}

fn op_set(context: &mut UpdateContext<'_>, path: &str, value_json: Json) -> Result<Json, String> {
    if path.is_empty() {
        return Err("path required".into());
    }
    let (obj, prop_path) = resolve_base_object(context, path)?;
    if prop_path.is_empty() {
        return Err("set requires a property path (e.g. root.x or so:name|allmoney)".into());
    }
    let parts: Vec<&str> = prop_path.split('.').filter(|p| !p.is_empty()).collect();
    let last = *parts.last().ok_or("property required")?;
    let parent_path = parts[..parts.len() - 1].join(".");

    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let parent = walk_props(&mut activation, obj, &parent_path)?;
    let new_val = json_to_value(&mut activation, &value_json)?;
    let last_name = AvmString::new_utf8(activation.gc(), last);
    parent
        .set_public_property(last_name, new_val, &mut activation)
        .map_err(|e| format!("set failed: {}", avm2_err(&e)))?;
    let read = parent
        .get_public_property(last_name, &mut activation)
        .map_err(|e| format!("readback failed: {}", avm2_err(&e)))?;
    *context.needs_render = true;
    Ok(json!({
        "ok": true,
        "path": path,
        "wrote": value_json,
        "readback": value_to_json_shallow(read)
    }))
}

fn op_call(
    context: &mut UpdateContext<'_>,
    path: &str,
    method: &str,
    args_json: Vec<Json>,
) -> Result<Json, String> {
    if method.is_empty() {
        return Err("method required".into());
    }
    // Resolve receiver + any {"$ref":"path"} args before activation borrow.
    let (obj, prop_path) = resolve_base_object(context, path)?;
    let mut ref_args: Vec<Option<(Avm2Object<'_>, String)>> = Vec::new();
    for a in &args_json {
        if let Some(ref_path) = a.get("$ref").and_then(|v| v.as_str()) {
            ref_args.push(Some(resolve_base_object(context, ref_path)?));
        } else {
            ref_args.push(None);
        }
    }

    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let receiver = walk_props(&mut activation, obj, &prop_path)?;
    let mut args: Vec<Avm2Value<'_>> = Vec::with_capacity(args_json.len());
    for (i, a) in args_json.iter().enumerate() {
        if let Some((o, p)) = &ref_args[i] {
            args.push(walk_props(&mut activation, *o, p)?);
        } else {
            args.push(json_to_value(&mut activation, a)?);
        }
    }
    let mname = AvmString::new_utf8(activation.gc(), method);
    let fa = FunctionArgs::from_slice(&args);
    let result = match receiver.call_public_property(mname, fa, &mut activation) {
        Ok(v) => v,
        Err(pub_err) => {
            // Private/protected methods: resolve by local name on the vtable, call via disp_id.
            let mut called: Option<Result<Avm2Value<'_>, Avm2Error<'_>>> = None;
            if let Some(obj) = receiver.as_object() {
                for (local_name, _ns, prop) in obj.vtable().resolved_traits().iter() {
                    if local_name != mname {
                        continue;
                    }
                    if let Property::Method { disp_id } = *prop {
                        called = Some(receiver.call_method_with_args(
                            disp_id,
                            FunctionArgs::from_slice(&args),
                            &mut activation,
                        ));
                        break;
                    }
                }
            }
            match called {
                Some(Ok(v)) => v,
                Some(Err(e)) => {
                    let detail = e.to_string(&mut activation);
                    return Err(format!(
                        "call failed (method={method} path={path}): {detail}"
                    ));
                }
                None => {
                    let detail = pub_err.to_string(&mut activation);
                    return Err(format!(
                        "call failed (method={method} path={path}): {detail}"
                    ));
                }
            }
        }
    };
    *context.needs_render = true;
    Ok(json!({
        "ok": true,
        "path": path,
        "method": method,
        "result": value_to_json_shallow(result)
    }))
}

fn op_find(context: &mut UpdateContext<'_>, keywords: &str, max_depth: usize, limit: usize) -> Json {
    let kws: Vec<String> = keywords
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let mut hits: Vec<Json> = Vec::new();

    // SharedObjects first (most useful for saves)
    let so_names: Vec<String> = context.avm2_shared_objects.keys().cloned().collect();
    for so_name in so_names {
        if hits.len() >= limit {
            break;
        }
        if let Ok(data) = resolve_so(context, &so_name) {
            let slots = list_slot_props(data, limit);
            for (n, v) in slots {
                if hits.len() >= limit {
                    break;
                }
                let nl = n.to_ascii_lowercase();
                if kws.iter().any(|k| nl.contains(k.as_str())) {
                    hits.push(json!({
                        "path": format!("so:{so_name}|{n}"),
                        "name": n,
                        "value": v,
                        "via": "shared_object_slot"
                    }));
                }
            }
            // skip SO enumerant walk — unsafe on some SharedObject data blobs
        }
    }

    // Display list walk (names + slot names only — no getter calls)
    let mut queue: VecDeque<(DisplayObject<'_>, usize, String)> = VecDeque::new();
    if let Some(root) = context.stage.root_clip() {
        queue.push_back((root, 0, "root".into()));
    }
    while let Some((dobj, depth, path)) = queue.pop_front() {
        if hits.len() >= limit {
            break;
        }
        let dn = dobj_name(dobj).to_ascii_lowercase();
        if kws.iter().any(|k| dn.contains(k.as_str())) {
            hits.push(json!({
                "path": path,
                "name": dobj_name(dobj),
                "value": null,
                "via": "display_name",
                "kind": dobj_kind(dobj)
            }));
        }
        if let Some(stage_obj) = dobj.object2() {
            let obj: Avm2Object<'_> = stage_obj.into();
            let slots = list_slot_props(obj, 40);
            for (n, v) in slots {
                if hits.len() >= limit {
                    break;
                }
                let nl = n.to_ascii_lowercase();
                if kws.iter().any(|k| nl.contains(k.as_str())) {
                    hits.push(json!({
                        "path": format!("{path}.{n}"),
                        "name": n,
                        "value": v,
                        "via": "slot"
                    }));
                }
            }
        }
        if depth < max_depth {
            if let Some(c) = dobj.as_container() {
                for child in c.iter_render_list() {
                    let cn = dobj_name(child);
                    queue.push_back((child, depth + 1, format!("{path}/{cn}")));
                }
            }
        }
    }

    json!({"ok": true, "keywords": kws, "hits": hits, "count": hits.len()})
}

fn op_list_so(context: &mut UpdateContext<'_>) -> Json {
    let names: Vec<String> = context.avm2_shared_objects.keys().cloned().collect();
    json!({"ok": true, "names": names, "count": names.len()})
}

fn op_get_so(context: &mut UpdateContext<'_>, name: &str, limit: usize) -> Result<Json, String> {
    if name.is_empty() {
        return Err("name required".into());
    }
    op_list_props(context, &format!("so:{name}"), limit)
}

fn op_set_so_prop(
    context: &mut UpdateContext<'_>,
    name: &str,
    prop: &str,
    value: Json,
) -> Result<Json, String> {
    if name.is_empty() || prop.is_empty() {
        return Err("name and prop required".into());
    }
    op_set(context, &format!("so:{name}|{prop}"), value)
}

/// Attach SharedObject instance (not .data) onto an object field.
fn op_bind_so(context: &mut UpdateContext<'_>, path: &str, so_name: &str) -> Result<Json, String> {
    if so_name.is_empty() {
        return Err("name = SharedObject key required".into());
    }
    let so = context
        .avm2_shared_objects
        .get(so_name)
        .copied()
        .or_else(|| {
            context
                .avm2_shared_objects
                .iter()
                .find(|(k, _)| k.ends_with(so_name) || k.contains(so_name))
                .map(|(_, v)| *v)
        })
        .ok_or_else(|| {
            format!(
                "SO not found: {so_name:?} have {:?}",
                context.avm2_shared_objects.keys().collect::<Vec<_>>()
            )
        })?;

    // path like root/thanksTanks.sharedOb
    let (obj, prop_path) = resolve_base_object(context, path)?;
    if prop_path.is_empty() {
        return Err("bind_so path needs a property (e.g. root/thanksTanks.sharedOb)".into());
    }
    let parts: Vec<&str> = prop_path.split('.').filter(|p| !p.is_empty()).collect();
    let last = *parts.last().ok_or("property required")?;
    let parent_path = parts[..parts.len() - 1].join(".");
    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let parent = walk_props(&mut activation, obj, &parent_path)?;
    let last_name = AvmString::new_utf8(activation.gc(), last);
    let so_val: Avm2Value<'_> = Avm2Object::from(so).into();
    parent
        .set_public_property(last_name, so_val, &mut activation)
        .map_err(|e| format!("bind_so set failed: {}", avm2_err(&e)))?;
    *context.needs_render = true;
    Ok(json!({
        "ok": true,
        "bound": true,
        "path": path,
        "so": so.name(),
    }))
}

/// Dump resolved vtable traits (real slot_ids, private + public) with live values.
fn op_list_slots(
    context: &mut UpdateContext<'_>,
    path: &str,
    limit: usize,
) -> Result<Json, String> {
    let (base, base_prop) = resolve_base_object(context, path)?;
    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let obj_val = walk_props(&mut activation, base, &base_prop)?;
    let obj = obj_val
        .as_object()
        .ok_or_else(|| "list_slots path is not an object".to_string())?;

    let mut traits_out: Vec<Json> = Vec::new();
    for (local_name, ns, prop) in obj.vtable().resolved_traits().iter() {
        if traits_out.len() >= limit {
            break;
        }
        let local = local_name.to_string();
        let ns_s = ns.as_uri_opt().map(|u| u.to_string()).unwrap_or_default();
        let (kind, slot_id) = match *prop {
            Property::Slot { slot_id } => ("Slot", Some(slot_id)),
            Property::ConstSlot { slot_id } => ("Const", Some(slot_id)),
            Property::Method { disp_id } => {
                traits_out.push(json!({
                    "name": local,
                    "ns": ns_s,
                    "kind": "Method",
                    "disp_id": disp_id,
                }));
                continue;
            }
            Property::Virtual { get, set } => {
                traits_out.push(json!({
                    "name": local,
                    "ns": ns_s,
                    "kind": "Virtual",
                    "get": get,
                    "set": set,
                }));
                continue;
            }
        };
        let mut entry = json!({
            "name": local,
            "ns": ns_s,
            "kind": kind,
            "slot_id": slot_id,
        });
        if let Some(id) = slot_id {
            entry["value"] = value_to_json_shallow(obj.get_slot(id));
        }
        traits_out.push(entry);
    }

    Ok(json!({
        "ok": true,
        "path": path,
        "class": class_name_of(obj),
        "traits": traits_out,
        "count": traits_out.len(),
        "note": "resolved vtable slots (includes private)"
    }))
}

/// Write a named slot (public or private) on an object via resolved vtable slot_id.
fn op_set_slot(
    context: &mut UpdateContext<'_>,
    obj_path: &str,
    field: &str,
    from_path: Option<&str>,
    value: Option<Json>,
) -> Result<Json, String> {
    if obj_path.is_empty() || field.is_empty() {
        return Err("set_slot needs path (object) and prop (field name)".into());
    }

    let (base, base_prop) = resolve_base_object(context, obj_path)?;
    let from_resolved = if let Some(fp) = from_path.filter(|s| !s.is_empty()) {
        Some(resolve_base_object(context, fp)?)
    } else {
        None
    };

    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let obj_val = walk_props(&mut activation, base, &base_prop)?;
    let obj = obj_val
        .as_object()
        .ok_or_else(|| "set_slot path is not an object".to_string())?;

    let new_val = if let Some((fo, fp)) = from_resolved {
        walk_props(&mut activation, fo, &fp)?
    } else if let Some(v) = value.as_ref() {
        json_to_value(&mut activation, v)?
    } else {
        return Err("set_slot needs value or name=$ref source path".into());
    };

    // Use resolved vtable — ABC trait slot_ids are often 0 (auto-assign).
    let mut found: Option<usize> = None;
    let mut near: Vec<String> = Vec::new();
    for (local_name, _ns, prop) in obj.vtable().resolved_traits().iter() {
        let local = local_name.to_string();
        if local == field {
            match *prop {
                Property::Slot { slot_id } | Property::ConstSlot { slot_id } => {
                    found = Some(slot_id);
                    break;
                }
                Property::Method { .. } => near.push(format!("{local}=Method")),
                Property::Virtual { .. } => near.push(format!("{local}=Virtual")),
            }
        } else if local.contains(field) || field.contains(local.as_str()) {
            let k = match *prop {
                Property::Slot { slot_id } => format!("Slot#{slot_id}"),
                Property::ConstSlot { slot_id } => format!("Const#{slot_id}"),
                Property::Method { .. } => "Method".into(),
                Property::Virtual { .. } => "Virtual".into(),
            };
            near.push(format!("{local}={k}"));
        }
    }

    let slot_id = found.ok_or_else(|| {
        format!(
            "slot {field:?} not found on {} (near={near:?})",
            class_name_of(obj)
        )
    })?;

    obj.set_slot(slot_id, new_val, &mut activation)
        .map_err(|e| format!("set_slot failed: {}", avm2_err(&e)))?;
    *context.needs_render = true;

    let rb = obj.get_slot(slot_id);
    Ok(json!({
        "ok": true,
        "path": obj_path,
        "prop": field,
        "slot_id": slot_id,
        "readback": value_to_json_shallow(rb)
    }))
}

fn op_set_ref(context: &mut UpdateContext<'_>, to_path: &str, from_path: &str) -> Result<Json, String> {
    if to_path.is_empty() || from_path.is_empty() {
        return Err("set_ref needs path (target) and name (source object path)".into());
    }
    // Resolve both paths before creating activation (mutable borrow of context).
    let (from_obj, from_prop) = resolve_base_object(context, from_path)?;
    let (to_obj, to_prop) = resolve_base_object(context, to_path)?;
    if to_prop.is_empty() {
        return Err("target path needs a property".into());
    }
    let parts: Vec<&str> = to_prop.split('.').filter(|p| !p.is_empty()).collect();
    let last = *parts.last().ok_or("property required")?;
    let parent_path = parts[..parts.len() - 1].join(".");

    let domain = context.avm2.stage_domain();
    let mut activation = Activation::from_domain(context, domain);
    let from_val = walk_props(&mut activation, from_obj, &from_prop)?;
    if from_val.as_object().is_none() {
        return Err("source path is not an object".into());
    }
    let parent = walk_props(&mut activation, to_obj, &parent_path)?;
    let last_name = AvmString::new_utf8(activation.gc(), last);
    parent
        .set_public_property(last_name, from_val, &mut activation)
        .map_err(|e| format!("set_ref failed: {}", avm2_err(&e)))?;
    *context.needs_render = true;
    Ok(json!({"ok": true, "from": from_path, "to": to_path}))
}

fn bytes_to_b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    if i < bytes.len() {
        let rem = bytes.len() - i;
        let b0 = bytes[i] as u32;
        let b1 = if rem > 1 { bytes[i + 1] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if rem == 1 {
            out.push('=');
            out.push('=');
        } else {
            out.push(T[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
    }
    out
}

fn b64_to_bytes(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("bad base64 char: {}", c as char)),
        }
    }
    let s: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !s.len().is_multiple_of(4) {
        return Err("base64 length".into());
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for chunk in s.chunks_exact(4) {
        let (a, b, c, d) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        let n = ((val(a)? as u32) << 18)
            | ((val(b)? as u32) << 12)
            | (if c == b'=' {
                0
            } else {
                val(c)? as u32
            } << 6)
            | if d == b'=' { 0 } else { val(d)? as u32 };
        out.push((n >> 16) as u8);
        if c != b'=' {
            out.push((n >> 8) as u8);
        }
        if d != b'=' {
            out.push(n as u8);
        }
    }
    Ok(out)
}

fn op_storage_get(context: &mut UpdateContext<'_>, key: &str) -> Json {
    if key.is_empty() {
        return json!({"ok": false, "error": "key required"});
    }
    match context.storage.get(key) {
        Some(bytes) => json!({
            "ok": true,
            "key": key,
            "found": true,
            "len": bytes.len(),
            "base64": bytes_to_b64(&bytes)
        }),
        None => json!({"ok": true, "key": key, "found": false}),
    }
}

fn op_storage_put(context: &mut UpdateContext<'_>, key: &str, b64: &str) -> Json {
    if key.is_empty() {
        return json!({"ok": false, "error": "key required"});
    }
    let bytes = match b64_to_bytes(b64) {
        Ok(b) => b,
        Err(e) => return json!({"ok": false, "error": format!("base64: {e}")}),
    };
    let ok = context.storage.put(key, &bytes);
    json!({"ok": ok, "key": key, "len": bytes.len()})
}
