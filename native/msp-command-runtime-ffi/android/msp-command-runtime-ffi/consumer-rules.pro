# JNI entry points are looked up by their generated names and must remain
# available in release AARs. Kotlin external declarations are kept with the
# owning bridge class.
-keep class com.reados.msp.commandruntime.MspCommandRuntimeFfiNative { *; }
-keep class com.reados.msp.commandruntime.MspFfiByteBuffer { *; }
