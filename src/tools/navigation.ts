import {categories, isToolId, type CategoryId, type ToolId} from './registry'
export type NavigationState={schema_version:1;category:CategoryId;tool:ToolId;favorites:ToolId[];scroll:Record<CategoryId,number>}
export const emptyNavigation=():NavigationState=>({schema_version:1,category:'common',tool:'check',favorites:[],scroll:{common:0,expert:0,utilities:0}})
export function readNavigation():NavigationState{
 const state=emptyNavigation()
 try{
  const data=JSON.parse(localStorage.getItem('hopkit-navigation')??'null')
  if(!data||data.schema_version!==1)return state
  if(isToolId(data.tool)){state.tool=data.tool;if(categories.some(c=>c.id===data.category))state.category=data.category}
  state.favorites=Array.isArray(data.favorites)?[...new Set<ToolId>(data.favorites.filter(isToolId))]:[]
  for(const {id} of categories)if(Number.isFinite(data.scroll?.[id])&&data.scroll[id]>=0)state.scroll[id]=Math.min(data.scroll[id],100000)
 }catch{/* Corrupt or blocked storage must not disable navigation. */}
 return state
}
export function writeNavigation(state:NavigationState):boolean{try{localStorage.setItem('hopkit-navigation',JSON.stringify(state));return true}catch{return false}}
