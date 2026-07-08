use iced::Color;
use kdl::{KdlNode, KdlValue};

use crate::tokens::ThemeTokens;

pub(super) fn parse_color_value(
    name: &str,
    value: &KdlValue,
    tokens: &ThemeTokens,
) -> Result<Color, String> {
    let source = value
        .as_string()
        .ok_or_else(|| format!("{name}: expected color string"))?;
    parse_color_expression(source, tokens).map_err(|error| format!("{name}: {error}"))
}

pub(super) fn parse_color_expression(source: &str, tokens: &ThemeTokens) -> Result<Color, String> {
    let source = source.trim();
    if let Some(token) = source.strip_prefix('$') {
        return theme_color(tokens, token).ok_or_else(|| format!("unknown color token `{source}`"));
    }
    if let Some(args) = source
        .strip_prefix("mix(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let parts = args.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(format!("invalid mix expression `{source}`"));
        }
        let base = parse_color_expression(parts[0], tokens)?;
        let overlay = parse_color_expression(parts[1], tokens)?;
        let amount = parts[2]
            .parse::<f32>()
            .map_err(|_| format!("invalid mix amount in `{source}`"))?;
        return Ok(crate::classes::mix_color(base, overlay, amount));
    }
    parse_color_literal(source)
}

pub(super) fn parse_color_literal(source: &str) -> Result<Color, String> {
    let source = source.trim();
    if let Some(args) = source
        .strip_prefix("rgba(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let values = args
            .split(',')
            .map(|part| part.trim().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("invalid rgba color `{source}`"))?;
        if values.len() != 4 {
            return Err(format!("invalid rgba color `{source}`"));
        }
        return Ok(Color::from_rgba8(
            values[0].clamp(0.0, 255.0) as u8,
            values[1].clamp(0.0, 255.0) as u8,
            values[2].clamp(0.0, 255.0) as u8,
            values[3].clamp(0.0, 1.0),
        ));
    }
    let hex = source
        .strip_prefix('#')
        .ok_or_else(|| format!("invalid color `{source}`"))?;
    if hex.len() != 6 && hex.len() != 8 {
        return Err(format!("invalid color `{source}`"));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| format!("invalid color `{source}`"))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| format!("invalid color `{source}`"))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| format!("invalid color `{source}`"))?;
    let a = if hex.len() == 8 {
        f32::from(
            u8::from_str_radix(&hex[6..8], 16).map_err(|_| format!("invalid color `{source}`"))?,
        ) / 255.0
    } else {
        1.0
    };
    Ok(Color::from_rgba8(r, g, b, a))
}

fn theme_color(tokens: &ThemeTokens, token: &str) -> Option<Color> {
    Some(match token {
        "background" => tokens.background,
        "surface" => tokens.surface,
        "surface_raised" => tokens.surface_raised,
        "text_primary" => tokens.text_primary,
        "text_secondary" => tokens.text_secondary,
        "accent" => tokens.accent,
        "border" => tokens.border,
        "error" => tokens.error,
        "canvas" => tokens.canvas,
        "placeholder" => tokens.placeholder,
        "focus" => tokens.focus,
        "shadow" => tokens.shadow,
        _ => return None,
    })
}

pub(super) fn node_string_arg<'a>(
    name: &str,
    node: &'a KdlNode,
    index: usize,
) -> Result<&'a str, String> {
    node.get(index)
        .and_then(KdlValue::as_string)
        .ok_or_else(|| {
            format!(
                "{name}: node `{}` missing string argument {index}",
                node.name().value()
            )
        })
}

pub(super) fn node_f32_arg(name: &str, node: &KdlNode, index: usize) -> Result<f32, String> {
    node.get(index)
        .map(|value| value_as_f32(name, value))
        .transpose()?
        .ok_or_else(|| {
            format!(
                "{name}: node `{}` missing numeric argument {index}",
                node.name().value()
            )
        })
}

pub(super) fn node_usize_arg(name: &str, node: &KdlNode, index: usize) -> Result<usize, String> {
    let value = node.get(index).ok_or_else(|| {
        format!(
            "{name}: node `{}` missing integer argument {index}",
            node.name().value()
        )
    })?;
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    usize::try_from(*value).map_err(|_| format!("{name}: expected non-negative integer"))
}

pub(super) fn value_as_f32(name: &str, value: &KdlValue) -> Result<f32, String> {
    let number = match value {
        KdlValue::Integer(value) => *value as f32,
        KdlValue::Float(value) => *value as f32,
        _ => return Err(format!("{name}: expected numeric value")),
    };
    if !number.is_finite() || number < 0.0 {
        return Err(format!("{name}: expected finite non-negative number"));
    }
    Ok(number)
}

pub(super) fn value_as_u16(name: &str, value: &KdlValue) -> Result<u16, String> {
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    u16::try_from(*value).map_err(|_| format!("{name}: expected integer from 0 to 65535"))
}

pub(super) fn value_as_usize(name: &str, value: &KdlValue) -> Result<usize, String> {
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    usize::try_from(*value).map_err(|_| format!("{name}: expected non-negative integer"))
}

pub(super) fn value_as_u32(name: &str, value: &KdlValue) -> Result<u32, String> {
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    u32::try_from(*value).map_err(|_| format!("{name}: expected non-negative integer"))
}

pub(super) fn parse_font_weight(name: &str, value: &str) -> Result<iced::font::Weight, String> {
    match value {
        "regular" | "normal" => Ok(iced::font::Weight::Normal),
        "medium" => Ok(iced::font::Weight::Medium),
        "semibold" | "semi_bold" => Ok(iced::font::Weight::Semibold),
        "bold" => Ok(iced::font::Weight::Bold),
        other => Err(format!("{name}: unsupported font weight `{other}`")),
    }
}
