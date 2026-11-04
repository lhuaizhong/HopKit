<script setup lang="ts">
import {computed} from 'vue'

const props=defineProps<{id:string;en:boolean;config:{ipv4?:boolean;ipv6?:boolean}}>()
const family=computed({
 get:()=>props.config.ipv4?'ipv4':props.config.ipv6?'ipv6':'auto',
 set:(value:string)=>{props.config.ipv4=value==='ipv4';props.config.ipv6=value==='ipv6'}
})
</script>

<template>
 <label :for="id">{{en?'Address family':'地址类型'}}</label>
 <select :id="id" v-model="family">
  <option value="auto">{{en?'Auto':'自动'}}</option>
  <option value="ipv4">IPv4</option>
  <option value="ipv6">IPv6</option>
 </select>
</template>
