export function unorm_to_percent(v)
{
 let p = (v * 100).toFixed(2)
 let length = p.length
 let padding_length = 7 - length
 return ' '.repeat(padding_length) + p + '%'
}

export function in_range(n, min, max)
{
 return min <= n && n < max
}