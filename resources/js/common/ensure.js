// condition が false の場合に message を console.error しつつ throw する
export function ensure(condition, message)
{
 if (!condition)
 {
  console.error(message)
  throw message
 }
}

export default ensure
