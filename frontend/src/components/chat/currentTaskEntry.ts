interface TaskGraphLookupResult { ok:boolean;data?:{graph_id?:string} }
export async function resolveCurrentTaskDestination(_activeTaskId:string|null,_loadGraph:(id:string)=>Promise<TaskGraphLookupResult>):Promise<string>{return "/connectors";}
