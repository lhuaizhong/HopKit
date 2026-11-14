<script setup lang="ts">
import {computed,nextTick,ref} from 'vue'
import {categories,tools,searchTools,categoryOf,displayName,type ToolId,type CategoryId} from '../../tools/registry'
import type {NavigationState} from '../../tools/navigation'
const props=defineProps<{en:boolean;state:NavigationState;activeTools:string[]}>()
const emit=defineEmits<{select:[id:ToolId];change:[];tasks:[]}>()
const query=ref(''),list=ref<HTMLElement|null>(null)
const t=(zh:string,en:string)=>props.en?en:zh
const items=computed(()=>query.value.trim()?searchTools(query.value):tools.filter(t=>t.category===props.state.category))
const activeCount=(category:CategoryId)=>props.activeTools.filter(id=>tools.some(tool=>tool.id===id&&tool.category===category)).length
const favorites=computed(()=>props.state.favorites.map(id=>tools.find(t=>t.id===id)!).filter(Boolean))
function scroll(){if(!query.value.trim()&&list.value){props.state.scroll[props.state.category]=list.value.scrollTop;emit('change')}}
async function category(id:CategoryId){scroll();props.state.category=id;query.value='';emit('change');await nextTick();if(list.value)list.value.scrollTop=props.state.scroll[id]}
async function select(id:ToolId){scroll();props.state.category=categoryOf(id);query.value='';emit('select',id);emit('change');await nextTick();if(list.value)list.value.scrollTop=props.state.scroll[props.state.category];document.querySelector<HTMLElement>('main h1')?.focus({preventScroll:true})}
function favorite(id:ToolId){props.state.favorites=props.state.favorites.includes(id)?props.state.favorites.filter(t=>t!==id):[...props.state.favorites,id];emit('change')}
nextTick(()=>{if(list.value)list.value.scrollTop=props.state.scroll[props.state.category]})
</script>
<template>
 <div class="tool-navigation">
  <label for="tool-search" class="nav-search-label">{{t('查找工具','Find a tool')}}</label>
  <input id="tool-search" v-model="query" type="search" :placeholder="t('搜索名称或协议…','Search name or protocol…')" @keydown.esc="query=''" @keydown.enter.prevent="items[0]&&select(items[0].id)">
  <div class="category-switch" role="group" :aria-label="t('工具分类','Tool categories')">
   <button v-for="c in categories" :key="c.id" type="button" :aria-pressed="state.category===c.id" @click="category(c.id)">{{en?c.label[1]:c.label[0]}}<span v-if="activeCount(c.id)"> · {{activeCount(c.id)}}</span></button>
  </div>
  <button type="button" class="navigation-tasks" @click="emit('tasks')">{{t('任务中心','Task center')}} · {{activeTools.length}} {{t('项运行中','active')}}</button>
  <nav ref="list" :aria-label="t('工具导航','Tool navigation')" @scroll.passive="scroll">
   <template v-if="favorites.length&&!query.trim()"><div class="nav-label">{{t('收藏','Favorites')}}</div><div v-for="item in favorites" :key="'favorite-'+item.id" class="tool-nav-row"><button type="button" class="nav-item" :class="{active:state.tool===item.id}" :aria-current="state.tool===item.id?'page':undefined" @click="select(item.id)"><span class="nav-glyph">{{item.glyph}}</span><span><b>{{displayName(item,en)}}</b><small>{{en?item.sub[1]:item.sub[0]}}</small></span></button><button type="button" class="favorite-toggle" :aria-label="t('取消收藏 ','Remove favorite ')+displayName(item,en)" @click="favorite(item.id)">★</button></div></template>
   <div class="nav-label">{{query.trim()?t('全部分类搜索结果','Results from all categories'):t('工具列表','Tools')}}</div>
   <p v-if="!items.length" class="navigation-empty" role="status">{{query.trim()?t('没有匹配的工具','No matching tools'):t('此分类暂无已实现工具。新工具将在验收后加入。','Tools will appear here as they become available.')}}</p>
   <div v-for="item in items" :key="item.id" class="tool-nav-row">
    <button type="button" :class="['nav-item',{active:state.tool===item.id}]" :aria-current="state.tool===item.id?'page':undefined" @click="select(item.id)"><span class="nav-glyph">{{item.glyph}}</span><span><b>{{displayName(item,en)}}</b><small>{{en?item.sub[1]:item.sub[0]}}</small><small v-if="query.trim()">{{en?categories.find(c=>c.id===item.category)!.label[1]:categories.find(c=>c.id===item.category)!.label[0]}} · {{en?item.desc[1]:item.desc[0]}}</small></span><i v-if="activeTools.includes(item.id)" class="live-dot"></i></button>
    <button type="button" class="favorite-toggle" :aria-pressed="state.favorites.includes(item.id)" :aria-label="(state.favorites.includes(item.id)?t('取消收藏 ','Remove favorite '):t('收藏工具 ','Favorite tool '))+displayName(item,en)" @click="favorite(item.id)">{{state.favorites.includes(item.id)?'★':'☆'}}</button>
   </div>
  </nav>
 </div>
</template>
<style scoped>
.tool-navigation{display:flex;flex-direction:column;min-height:0;flex:1;gap:10px}.nav-search-label{font-size:12px;margin:0;color:#43536a}#tool-search{width:100%;min-width:0;padding:9px;border:1px solid #8b9bb0}.category-switch{display:flex;flex-direction:column;gap:4px}.category-switch button,.navigation-tasks{min-height:32px;border:1px solid #b6c4d4;background:#fff;color:#43536a;text-align:left;padding:6px 9px}.category-switch [aria-pressed=true]{background:#eaf2ff;color:#174b94;border-color:#1756a9}.navigation-tasks{color:#174b94}.tool-nav-row{display:flex;align-items:center;min-width:0}.nav-item{flex:1;min-width:0;margin:0;padding:10px 5px;gap:8px;width:auto}.nav-item>span:nth-child(2){min-width:0}.nav-item b,.nav-item small{overflow-wrap:anywhere;white-space:normal}.favorite-toggle{flex:none;width:28px;padding:4px 0;background:transparent;border:0;color:#82601b;font-size:19px}.navigation-empty{font-size:13px;padding:12px 8px;line-height:1.65;color:#43536a}.nav-label{padding:8px;font-size:11px}nav{flex:1;min-height:0;overflow-y:auto}.nav-glyph{flex-shrink:0;width:22px;font-size:21px}button:focus-visible,#tool-search:focus-visible{outline:3px solid #1756a9;outline-offset:-2px}
:root[data-theme=readable] .category-switch button,:root[data-theme=readable] .navigation-tasks,:root[data-theme=readable] .nav-search-label,:root[data-theme=readable] .nav-label,:root[data-theme=readable] .navigation-empty{font-size:14px} :root[data-theme=readable] .favorite-toggle{min-height:40px}
</style>
