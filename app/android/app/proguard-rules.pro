# JNA looks up its own classes and the UniFFI interfaces by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class dev.tickalt.core.** { *; }
-dontwarn java.awt.**
