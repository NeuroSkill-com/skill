# Ventana principal
La ventana principal es el panel principal. Muestra datos de EEG en tiempo real, el estado del dispositivo y la calidad de la señal. Siempre está visible en la barra de menú.

## Héroe de estado
La tarjeta superior muestra el estado de conexión en vivo de su dispositivo BCI. Un anillo de color y una insignia indican si el dispositivo está desconectado, escaneando, conectado o si Bluetooth está apagado. Cuando está conectado, se muestran el nombre del dispositivo, el número de serie y la dirección MAC (haga clic para revelar/ocultar).

## Batería
Una barra de progreso que muestra la carga actual de la batería del auricular BCI conectado. El color cambia de verde (alto), pasando por ámbar y rojo (bajo), a medida que cae la carga.

## Calidad de la señal
Cuatro puntos codificados por colores: uno por electrodo EEG (TP9, AF7, AF8, TP10). Verde = buen contacto con la piel y poco ruido. Amarillo = regular (algún artefacto). Rojo = deficiente (alto ruido/electrodo suelto). Gris = sin señal. La calidad se calcula a partir de una ventana RMS móvil sobre los datos sin procesar del EEG.

## Cuadrícula de canales EEG
Cuatro tarjetas que muestran el último valor de muestra (en µV) para cada canal, codificadas por colores para que coincidan con el gráfico de formas de onda a continuación.

## Tiempo de actividad y muestras
El tiempo de actividad cuenta los segundos desde que comenzó la sesión actual. Muestras es el número total de muestras de EEG sin procesar recibidas del auricular en esta sesión.

## Grabación CSV
Cuando está conectado, un indicador REC muestra el nombre del archivo CSV que se está escribiendo en {dataDir}/. Las muestras de EEG sin procesar (sin filtrar) se guardan continuamente: un archivo por sesión.

## Poderes de banda
Un gráfico de barras en vivo que muestra la potencia relativa en cada banda de frecuencia de EEG estándar: Delta (1 a 4 Hz), Theta (4 a 8 Hz), Alfa (8 a 13 Hz), Beta (13 a 30 Hz) y Gamma (30 a 50 Hz). Actualizado a ~4 Hz desde una FFT con ventana Hann de 512 muestras. Cada canal se muestra por separado.

## Asimetría Alfa Frontal (FAA)
Un medidor anclado en el centro que muestra el índice de asimetría frontal alfa en tiempo real: ln(AF8 α) − ln(AF7 α). Los valores positivos indican un mayor poder alfa frontal derecho, que se asocia con la motivación de aproximación del hemisferio izquierdo. Los valores negativos indican tendencia a la retirada. El valor se suaviza con una media móvil exponencial y normalmente oscila entre −1 y +1. FAA se almacena junto con cada época de incrustación de 5 segundos en eeg.sqlite.

## Formas de onda EEG
Un gráfico de desplazamiento en el dominio del tiempo de la señal EEG filtrada para todos los canales. Debajo de cada forma de onda hay una cinta de espectrograma que muestra el contenido de frecuencia a lo largo del tiempo. El gráfico muestra los ~4 segundos de datos más recientes.

## Utilización de GPU
Un pequeño gráfico en la parte superior de la ventana principal que muestra la utilización del codificador y decodificador de GPU. Visible solo mientras el codificador de incrustación de EEG está activo. Ayuda a verificar que la canalización wgpu está funcionando.

# Estados del icono de la bandeja

## Gris: desconectado
Bluetooth está activado; no hay ningún dispositivo BCI conectado.

## Ámbar: escaneado
Buscando un dispositivo BCI o intentando conectarse.

## Verde: conectado
Transmisión de datos de EEG en vivo desde su dispositivo BCI.

## Rojo: Bluetooth desactivado
La radio Bluetooth está apagada. No es posible escanear ni conectar.

# Comunidad
Únase a la comunidad de Discord de NeuroSkill para hacer preguntas, compartir comentarios y conectarse con otros usuarios y desarrolladores.
