export function connectionPresentation(configured:boolean,available:boolean,result?:string,dirty=false) {
  if(dirty)return {label:'有未保存修改',tone:'warning' as const};
  if(result)return result==='连接正常'||result==='试听已开始'
    ? {label:'连接正常',tone:'success' as const}
    : {label:'当前不可用',tone:'danger' as const};
  if(!configured)return {label:'未配置',tone:'default' as const};
  if(!available)return {label:'当前不可用',tone:'warning' as const};
  return {label:'已保存 · 未测试',tone:'default' as const};
}
